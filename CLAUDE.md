# `df monitor` 设计规格说明书

> 这是一个面向 Claude Code 的实现规格文档。目标是用 Rust 实现 `df` CLI 的 `monitor` 子命令——一个查询应用监控指标的终端 UI，类似 `k9s` / `btop` 的体验。

---

## 0. 项目目标与边界

### 0.1 这是什么

`df monitor <app>` 是 `df` CLI 工具的一个子命令，用于在终端中查看一个应用的实时监控指标。设计灵感来自 `htop`、`btop`、`k9s`、`Grafana`。

### 0.2 核心使用场景

- **release-watch**：发布灰度时盯盘 30min~1h，关注 5xx、P99 是否劣化
- **on-call**：告警触发后快速进入查看 1-6h 范围
- **AI Agent 调用**：通过 `--json` 给上游 Agent 喂结构化数据
- **大盘**：竖屏副屏长期挂着实时刷新

### 0.3 边界（不做的事）

- 不实现告警规则编辑（用 Grafana）
- 不做指标定义（数据从外部 HTTP API 拉，schema 已定）
- 不存储历史（外部 SLS/Prometheus 已有）
- 不支持带鱼屏（21:9 / 32:9）专门优化

### 0.4 推荐技术栈

| 模块 | 选型 | 说明 |
|------|------|------|
| TUI | `ratatui` 0.28+ + `crossterm` 0.28+ | 内置 double-buffer diff，避免全屏重绘 |
| 异步 | `tokio` 1.x | 后台拉数据 + 前台渲染解耦 |
| HTTP | `reqwest` + `serde_json` | 数据 API 客户端 |
| 配置 | `figment` + `toml` | XDG 路径下读 `~/.config/df/config.toml` |
| 颜色 | 直接写 truecolor (RGB) | 不用 palette 库，简单 lerp 函数即可 |
| 参数 | `clap` 4.x derive | 子命令风格 |
| 日志 | `tracing` + `tracing-subscriber` | 写到 `~/.cache/df/monitor.log`，不污染 TUI |

---

## 1. CLI 命令规格

### 1.1 基本调用

```bash
df monitor <app>                              # 默认 3h，左右两列总览
df monitor <app> --watch                      # 60s 自动刷新
df monitor <app> --json                       # 输出 JSON，不进 TUI
df monitor <app> --since=24h                  # 相对时间
df monitor <app> --from="2026-05-18T10:00" --to="2026-05-18T12:00"
df monitor <app> --pod=pod-a,pod-b            # 指定 Pod（逗号分隔或重复传）
df monitor <app> --metric=cpu                 # 单图模式
df monitor <app> --metric=cpu,memory          # 多指标并列（罕见，默认全总览）
df monitor <app> --interval=30s               # 自定义刷新间隔，配合 --watch
```

### 1.2 参数定义（clap derive）

```rust
#[derive(Parser)]
pub struct MonitorArgs {
    /// Application name
    pub app: String,

    /// Auto-refresh
    #[arg(short, long)]
    pub watch: bool,

    /// Refresh interval (only with --watch). Default: 60s
    #[arg(long, default_value = "60s")]
    pub interval: humantime::Duration,

    /// Output JSON instead of TUI
    #[arg(short, long)]
    pub json: bool,

    /// Time range: relative duration (e.g., 1h, 3h, 24h)
    #[arg(long, default_value = "3h")]
    pub since: humantime::Duration,

    /// Absolute start time (overrides --since)
    #[arg(long)]
    pub from: Option<chrono::DateTime<chrono::Local>>,

    /// Absolute end time (default: now)
    #[arg(long)]
    pub to: Option<chrono::DateTime<chrono::Local>>,

    /// Filter by pod name (repeatable or comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    pub pod: Vec<String>,

    /// Specific metric(s) to show in single-metric mode
    #[arg(short, long, value_delimiter = ',')]
    pub metric: Vec<String>,
}
```

### 1.3 退出码

| Code | 含义 |
|------|------|
| 0 | 正常退出（用户按 q / Ctrl+C） |
| 1 | 通用错误（参数/网络/JSON 解析） |
| 2 | 终端尺寸不足且不是 `--json` 模式 |
| 3 | 应用不存在 |
| 130 | 收到 SIGINT（兼容 Unix 习惯） |

---

## 2. 数据模型

### 2.1 输入数据

数据由后端 API 提供，**已经做过 Prometheus 转换**，前端只需 deserialize。响应格式：

```rust
#[derive(Deserialize, Clone, Debug)]
pub struct MonitorResponse {
    pub app: String,
    pub region: String,
    pub env: String,
    pub time_range: TimeRange,
    pub resolution_seconds: u32,    // 每个数据点的时间步长
    pub pods: Vec<PodInfo>,
    pub metrics: HashMap<MetricKind, MetricData>,
    pub events: Vec<Event>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TimeRange { pub from: DateTime<Utc>, pub to: DateTime<Utc> }

#[derive(Deserialize, Clone, Debug)]
pub struct PodInfo {
    pub name: String,
    pub status: String,           // "Running" / "Pending" / "CrashLoopBackOff"
    pub uptime_seconds: u64,
    pub restarts: u32,
    pub last_restart_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Qps,            // 流量
    Latency,        // 延迟
    ErrorRate,      // 错误率
    Upstream,       // 上游依赖耗时
    Cpu,            // CPU 使用率
    Memory,         // 内存
    Replicas,       // 实例数 + 重启
    Runtime,        // GC / goroutines / 线程
}

#[derive(Deserialize, Clone, Debug)]
pub struct MetricData {
    pub unit: String,             // "/s", "ms", "%", "bytes", ...
    /// 每条 series 是一条线（如 P50/P95/P99 或 pod-a/pod-b/pod-c）
    pub series: Vec<Series>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Series {
    pub label: String,            // "P99" / "pod-a" / "5xx"
    pub kind: SeriesKind,         // 用于决定颜色和图表形态
    pub aggregation: Aggregation, // 聚合方式（max / avg / sum / p99）
    pub across_pods: bool,        // 是否已跨 pod 聚合；false 表示单 pod 数据
    pub points: Vec<(DateTime<Utc>, f64)>,
}

#[derive(Deserialize, Clone, Debug)]
pub enum SeriesKind {
    Percentile(u8),  // 50, 95, 99
    StatusCode(u16), // 200, 400, 500 (堆叠柱状用)
    Pod(String),     // pod name
    Component(String), // "hsf" / "db" / "redis"
    Single,          // 单线
}

/// 聚合方式：在 panel 标题 / subtitle 中明示，避免误读
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    Max,        // 多 pod 取最大（关注风险时用，如 CPU/Memory/Latency P99）
    Avg,        // 多 pod 取平均（关注整体水位）
    Sum,        // 多 pod 求和（流量类）
    P50,
    P95,
    P99,
    Raw,        // 未聚合的单 pod 数据
}

#[derive(Deserialize, Clone, Debug)]
pub struct Event {
    pub at: DateTime<Utc>,
    pub kind: EventKind,
    pub message: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Restart, Deploy, AlertFired, AlertResolved, ScaleEvent
}
```

### 2.2 默认展示的 8 个 panel

按"4 黄金信号 + USE"组织。**默认 8 个，最少 6 个**（如果 Runtime 数据拿不到则隐藏）。

| 顺序 | Panel 标题 | MetricKind | 图表类型 | 默认 series | 默认聚合 |
|------|---------|-----------|---------|------------|---------|
| 1 | QPS by Status / RPM by Status | `Qps` | Stacked bar | 2xx/4xx/5xx | **sum**（跨 pod 求和） |
| 2 | Latency | `Latency` | Multi-line | P50/P95/P99 | **max**（取所有 pod 中最差） |
| 3 | Error Rate | `ErrorRate` | Single line | (4xx+5xx)/total % | **avg**（加权平均） |
| 4 | Upstream P99 | `Upstream` | Multi-line | HSF/DB/Redis | max |
| 5 | CPU Usage | `Cpu` | Multi-line | **max + avg 两条线** | 见 § 2.3 |
| 6 | Memory | `Memory` | Multi-line | **max + avg 两条线** | 见 § 2.3 |
| 7 | Replicas & Restarts | `Replicas` | **详情卡片**（非图表）| pod 表格 | n/a |
| 8 | Runtime | `Runtime` | Multi-line | GC pause / goroutines | max |

### 2.3 聚合策略与切换

**核心原则**：UI 永远要让用户知道"我在看 max 还是 avg"。具体规则：

#### 默认聚合

| 指标类型 | 默认聚合 | 理由 |
|---------|---------|------|
| 流量类（QPS、RPM、bytes/s） | `sum` | 流量是相加的，3 个 pod 各 100 QPS 总流量 300 QPS |
| 延迟类（P50/P95/P99） | `max` | 风险驱动：只要一个 pod 慢就值得关注 |
| 错误率 | `avg`（加权） | 整体客户感知 |
| CPU / Memory | **同时画 max 和 avg 两条线** | 既看风险也看水位 |
| GC pause / goroutines | `max` | 异常单 pod 信号 |

#### Max + Avg 双线渲染（关键交互）

CPU、Memory 这类"分布敏感"的指标，**默认画两条线**：
- `max` 用实线 + 亮色（如 `#ffb86c`）
- `avg` 用半透明线 + 同色系暗版（如 `#ffb86c` 50% alpha 混到 BG）

在 subtitle 显示两个值：`max 78% · avg 45%`。如果 `max - avg > 30%`（pod 间方差大），subtitle 追加 `⚠ uneven` 提示，鼓励用户切到 per-pod 视图。

#### 用户切换聚合方式

在任何 panel focus 时按 **`a`** 键循环切换聚合方式：

```
max → avg → sum → p95 → per-pod → max
```

切换后 panel 标题立即更新，例如 `CPU Usage (max)` → `CPU Usage (avg)`。`per-pod` 模式下不聚合，每个 pod 一条线（颜色按 pod 区分）。

#### 多 pod 数据缺失的兜底

如果后端 `Series.aggregation` 字段为空（旧数据 / API 没升级），按下表猜测：

| 检测条件 | 假设 |
|---------|------|
| `MetricKind == Qps` 且 `series.across_pods == true` | sum |
| `kind == Percentile(_)` | max |
| 其他且 `across_pods == true` | avg（**并在 subtitle 警告 "aggregation unknown"**） |
| `across_pods == false` | raw（单 pod 真实数据） |

#### 流量单位动态切换（QPS / RPM / K/s）

数据源给的是 **RPM**（每分钟请求数，分钟级采样，且已是多 pod 聚合）。展示时按时间范围内的**中位数**决定单位，整段时间轴用同一单位避免横跳：

```rust
fn format_traffic_rate(rpm: f64) -> (f64, &'static str) {
    let qps = rpm / 60.0;
    if qps < 1.0 { (rpm, "RPM") }
    else if qps < 1000.0 { (qps, "QPS") }
    else { (qps / 1000.0, "K/s") }
}

fn pick_traffic_unit(series: &[Series]) -> &'static str {
    // 用中位数决定，不用最大或最新（避免被尖峰带偏）
    let all_rpms: Vec<f64> = series.iter()
        .flat_map(|s| s.points.iter().map(|p| p.1))
        .collect();
    let median = percentile(&all_rpms, 50);
    format_traffic_rate(median).1
}
```

Panel 标题反映实际单位：`QPS by Status (1m avg)` 或 `RPM by Status`。
括号里的 `(1m avg)` 提示用户这是"分钟级采样除以 60 的平均值"，**不是真实瞬时 QPS**。

---

## 3. 响应式布局

### 3.1 布局档位

按终端 `(width, height)` 决定布局，每帧根据 `crossterm::terminal::size()` 重新计算（监听 `SIGWINCH`）。

```rust
pub enum LayoutTier {
    TooSmall,                            // < 60 宽 或 < 20 高
    Phone,                               // 60-99 宽 或 < 30 高（手机 SSH 横/竖屏）
    SingleColumn { panels: u8 },         // 单列滚动：100-130 宽（桌面窄窗口）
    TwoByFour,                           // 默认 2×4 八图：130-200 宽
    TwoByFourLarge,                      // 加大版：200-260 宽
    TwoByFourSidebar,                    // 2×4 + 事件流侧栏：≥ 260 宽
    Portrait,                            // 竖屏单列：height > width 且 height > 80
    SingleMetric,                        // --metric=xxx 单图模式（不受尺寸触发，由 CLI 参数触发）
}

impl LayoutTier {
    pub fn from_size(w: u16, h: u16, args: &MonitorArgs) -> Self {
        if !args.metric.is_empty() && args.metric.len() == 1 {
            return Self::SingleMetric;
        }
        if w < 60 || h < 20 { return Self::TooSmall; }
        if w < 100 || h < 30 { return Self::Phone; }       // 手机 SSH 落到这里
        if h > w && h > 80 { return Self::Portrait; }
        match w {
            0..=129 => Self::SingleColumn { panels: 8 },
            130..=199 => Self::TwoByFour,
            200..=259 => Self::TwoByFourLarge,
            _ => Self::TwoByFourSidebar,
        }
    }
}
```

### 3.2 各档位特征

| 档位 | 场景 | 布局 | Chart 内部高度 | Y 轴标签数 |
|------|------|------|--------------|----------|
| `TooSmall` | < 60 宽 | 居中提示框 | n/a | n/a |
| `Phone` | 手机 SSH（Termius/Blink） | **单 panel 全屏 + dot 指示器**（详见 § 3.4） | 6 行 | 3 |
| `SingleColumn` | 14" 半屏（桌面窄窗口）| 单列纵向 8 panel，上下滚动 | 4 行 | 3 |
| `TwoByFour` | **默认目标** 14" 全屏 / 16" 半屏 | 2 列 × 4 行 | 6 行 | 3 |
| `TwoByFourLarge` | 16" 全屏 / 24" 全屏 | 2 列 × 4 行（panel 加宽加高） | 9 行 | 5 |
| `TwoByFourSidebar` | 27"/32" 全屏 | 2 列 × 4 行 + 右侧 35 宽事件流侧栏 | 9 行 | 5 |
| `Portrait` | 27" 竖屏副屏 / iPad 竖屏 | 单列纵向 8 panel | 8 行 | 5 |
| `SingleMetric` | `--metric=xxx` | 顶部 KPI 卡 × 4 + 大图 + 右侧 pod 卡 × 3 | 22 行 | 5 |

### 3.3 太小提示

```
┌─ df monitor demo-app ────────────────────────────────────┐
│                                                          │
│   ⚠  Terminal too small                                  │
│                                                          │
│   df monitor needs at least  60 × 20                     │
│   your terminal is            48 × 16                    │
│                                                          │
│   Options:                                               │
│     • resize your terminal                               │
│     • view a single metric:  df monitor demo-app --metric=cpu │
│     • use JSON mode:         df monitor demo-app --json  │
│                                                          │
│   [r] retry    [q] quit                                  │
└──────────────────────────────────────────────────────────┘
```

退出码 2，不阻塞，按 `r` 重新检测尺寸（resize 后可用）。

### 3.4 Phone 档（手机端 SSH 专属）

**触发条件**：60 ≤ 宽 < 100 或 高 < 30

**目标设备**：iPhone 横屏（~80×24）、iPhone 竖屏（~40×40）、iPad mini 竖屏（~70×80）

**核心设计**：每次只显示 **1 个 panel** 占满屏幕，底部 dot indicator 显示总共 8 个 + 当前位置，上下方向键切换 panel（不是焦点移动，是**翻页**）。

#### 布局示意（iPhone 横屏 ~80×24）

```
┌─ df monitor demo-app · prod ───── 14:32 ─┐
│ ◆ Error Rate  ALERT  3.19%               │
│ ┄                                        │
│ 8.1% ┄                                   │
│              ⡆                           │
│          ⢠⡾⣦                           │
│ 4.5% ⢀⣀⣀⣠⠤⣄⣀⡀  ⣀⡀  ⣀⣀⢀⡀  ⣀     │
│      ⠈⠁⠈⠁⠈⠁⠈⠁⠉⠁⠈⠁⠉⠁⠉⠁⠉⠁⠉   │
│ 2.4% ┄                                   │
│                                          │
│ peaked 7.8% at 14:08                     │
│                                          │
│             •  •  ●  •  •  •  •  •       │
│             1  2  3  4  5  6  7  8       │
│                                          │
│ [↑↓] panel  [↵] detail  [w] watch  [q]   │
└──────────────────────────────────────────┘
```

#### iPhone 竖屏（~40×40）

宽度更窄，去掉 Y 轴中间标签（只留顶/底），footer 拆 2 行：

```
┌─ demo-app ─── 14:32 ─┐
│ ◆ Error Rate  ALERT  │
│ 3.19%                │
│                      │
│ 8.1%                 │
│         ⡆            │
│     ⢠⡾⣦            │
│ ⢀⣀⣀⣠⠤⣄⣀⡀  ⣀⡀  ⣀ │
│ ⠈⠁⠈⠁⠈⠁⠈⠁⠉⠁⠈⠁⠉⠁│
│ 2.4%                 │
│                      │
│ peak 7.8% @ 14:08    │
│                      │
│  • • ● • • • • •     │
│  1 2 3 4 5 6 7 8     │
│                      │
│ [↑↓] switch panel    │
│ [↵] detail  [q] quit │
└──────────────────────┘
```

#### Phone 档特殊规则

| 规则 | 说明 |
|------|------|
| Header 极简 | 仅保留 `app · env · time`，省掉 region/pods/range，需要时 `i` 键查看完整 header |
| 标题副信息 | 改为单行小字（"now X / peaked Y at Z"），不要复杂 legend |
| Legend 省略 | 单 series 不画 legend；多 series 在 subtitle 内联（"P99 187ms · P95 110ms"） |
| 网格 | 去掉 chart 内部网格点，只保留 Y 轴刻度 |
| 边框 | 用 `box::ROUNDED` 不变，但 padding 全部 0（手机像素稀缺） |
| dot indicator | 当前 panel 用 `●` 状态色，其他 `•` 用 `text_secondary` |
| 数字单位 | 优先用缩写：`1.2K/s` 而非 `1234 QPS`，`2.3G` 而非 `2.3 GB` |

#### Phone 档键盘

| 键 | 动作 |
|----|------|
| `↑` / `k` | 上一个 panel |
| `↓` / `j` | 下一个 panel |
| `←` / `→` | （预留，未来支持 time-pan） |
| `Enter` | 进当前 panel 的单图详情（沿用 SingleMetric 视图但用 Phone 布局） |
| `Esc` | 详情模式返回总览 |
| `w` | 切换 watch |
| `i` | 显示完整 header info（弹出 overlay） |
| `?` | help overlay |
| `q` / `Ctrl+C` | 退出 |

> **关于手势**：很多手机 SSH 客户端（Termius）支持把 swipe 映射到方向键，所以用户实际操作是"上滑下滑切 panel、双击进详情"。我们不做手势识别，只确保**纯键盘可用**。

#### Phone 档下的 SingleMetric 模式

`df monitor demo-app --metric=cpu` 在手机上要降级：

- KPI 卡片从 4 列变 **2×2 网格**（每张卡 ~30 宽）
- 侧栏 pod 卡片**折叠到主图下方**（垂直堆叠，每个变 1 行紧凑展示）
- 主图高度减半（10 行）
- 事件行折叠成单行可滑动

```
┌─ df monitor demo-app --metric=cpu ───────┐
│ ❯ CPU Usage                       14:32  │
│                                          │
│ ┌NOW──────────┐ ┌3H AVG──────────┐       │
│ │ 55.3%       │ │ 43.5%          │       │
│ │ max pod-c   │ │ avg 3 pods     │       │
│ └─────────────┘ └────────────────┘       │
│ ┌3H PEAK──────┐ ┌TREND 10m───────┐       │
│ │ 78.5%       │ │ ↑ 0.6%         │       │
│ │ 14:08:42    │ │ vs 30m ago     │       │
│ └─────────────┘ └────────────────┘       │
│                                          │
│ ◆ CPU Usage   a 42% · b 30% · c 55%      │
│ 95%                                      │
│                          ⢠                │
│ 55%   ⠉⠉⠒⠒⠒⠒⠒⠒⠒⢖⠒⠒⠒⠒⠒⠒⠒   │
│       ━━━━━━━━━━━━━━━━━━━━━━━━━━━   │
│ 35%   ⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉   │
│ 15%                                      │
│                                          │
│ ◉ pod-a 41.8% ↑1.2%  avg 41.9%           │
│ ◉ pod-b 29.6% ↓0.0%  avg 30.2%           │
│ ◉ pod-c 55.3% ↑0.6%  avg 58.3%           │
│                                          │
│ [esc] back  [w] watch  [q] quit          │
└──────────────────────────────────────────┘
```

---

## 4. 视觉规格（btop / k9s 质感）

### 4.1 颜色调色板（truecolor RGB）

| 角色 | Hex | 用途 |
|------|-----|------|
| `bg` | `#0d1117` | 默认背景（不强制设置，由用户终端主题决定，但 panel 内可强制） |
| `border_dim` | `#3a3a3a` | 普通 panel 边框 |
| `border_grid` | `#252525` | 网格线、分隔线 |
| `text_primary` | `#ffffff` | 正文 |
| `text_secondary` | `#5a5a5a` | label、单位、subtitle |
| `text_dim` | `#3a3a3a` | 极弱文字 |
| `accent_ok` | `#00d4aa` | OK 状态、主品牌色（命令名、LIVE 指示器）|
| `accent_warn` | `#ffb86c` | WARN 状态、倒计时 |
| `accent_alert` | `#ff6b6b` | ALERT 状态、错误尖峰 |
| `accent_info` | `#7c9eff` | 信息（pod-a、P50、HSF）|
| `accent_secondary` | `#ff79c6` | 次要 series（P99、DB）|

**配色策略**：边框颜色 = 该 panel 状态色（OK 用 `border_dim` 而非绿色，避免视觉噪声；WARN/ALERT 用对应警示色）。

### 4.2 字符库与字体

**字符**：所有图表用 Unicode Braille 块字符（`U+2800` 起，2×4 像素分辨率），堆叠柱状用 `▁▂▃▄▅▆▇█`，状态点用 `◉`（实心）`○`（空心）`◆`（强调）`●`（badge），分隔线 `─━│┊┄┆`。

**字体（推荐）**：
- **首选**：`JetBrainsMono Nerd Font`（你已用）— Braille 完整、字重清晰、有 ligature
- 备选：`MesloLGS Nerd Font`、`FiraCode Nerd Font`、`Iosevka`
- macOS 系统自带的 `Menlo` 也行，但 Braille 字符稍粗
- **避免**：`Courier New`（Braille 字符宽度不准）、`Hack` 早期版本（Braille 渲染有 bug）

文档需要提示用户：如果看到口字框 / 字符变形，是终端字体不支持 Braille，需要装 Nerd Font。

### 4.3 标题栏

```
╭─ ◆ Latency  WARN  P99 177ms · spike at 14:08 ─────────────╮
```

- 圆角边框 `box::ROUNDED`
- `◆` 状态点用 badge 色
- 标题正文用 `bold white`
- `WARN` badge 用 badge 色 bold
- 副标题用 `text_secondary`，提供"现在是什么 + 最值得关注的事"

### 4.4 图表渲染（关键技术细节）

**多线 area chart 算法**（非堆叠类指标，如 CPU、Latency、Memory）：

每条 series 独立画两层：
1. **edge 线**：曲线本身，亮色（edge_color）
2. **fade tail**：从曲线点向下 `fade_height` 个像素（4-6 个 cell），颜色按距离指数衰减到背景色

伪代码：
```rust
fn render_series(curve: &[f64], y_min: f64, y_max: f64,
                 edge_color: Rgb, fill_color: Rgb,
                 cells: &mut [[Cell]], z_index: u8) {
    let fade_px = FADE_HEIGHT * 4;  // 每 cell 4 px 高
    for (x_px, &value) in curve.iter().enumerate() {
        let y_curve = value_to_pixel(value, y_min, y_max);
        for dy in 0..fade_px {
            let y_px = y_curve + dy;
            if y_px >= H_PX { break; }
            let cell = &mut cells[y_px / 4][x_px / 2];
            cell.set_braille_dot(x_px % 2, y_px % 4);
            let color = if dy == 0 {
                edge_color
            } else {
                let t = (1.0 - dy as f32 / fade_px as f32).powf(1.5);
                lerp(BG, fill_color, t * 0.6)
            };
            // z-order：只有同层或更高层才覆盖
            if z_index >= cell.z_index {
                cell.color = color;
                cell.z_index = z_index;
            }
        }
    }
}
```

**关键点**：
- 不要画到 chart 底部（会互相完全遮挡），只画一段 fade tail
- 多 series 按平均值排序，**高的画在上层**（z_index 大），这样视觉上离观察者更近
- z_index 不是简单的"后画覆盖前画"，是"层级高的可以覆盖层级低的，反之不行"

**堆叠柱状图**（QPS by Status）：

逐柱遍历，每个 cell 行计算 [cell_bot, cell_top] 内每个 series 的占用区间，取顶部那个 series 的颜色，用 `▁▂▃▄▅▆▇█` 表示填充比例。

**网格线**：
- Y 轴的 label 行（top/q1/mid/q3/bot）在 chart 区域画一行淡淡的 `·`（每 5 列一个），颜色 `#1f1f1f`
- 不画垂直网格（太密）

### 4.5 KPI 卡片（单图模式）

```
┌◆ CURRENT       
│ 55.3%          
│ max · pod-c    
└────────────────
```

- 宽度 29 字符
- 高度 5 行
- box style：`box::SQUARE`（直角），border `#252525`
- 上左角的 ◆ 标记用 badge 色
- title 一行（小，`#5a5a5a` bold）
- 主值一行（大，badge 色 bold）
- sub 一行（小，`#5a5a5a`）

### 4.6 Replicas 详情卡（非图表 panel）

不画图，画一个紧凑表格：

```
  ◉ pod-a    Running       uptime 84d 6h    restarts 0      cpu 44%   mem 2.3G
  ◉ pod-b    Running       uptime 84d 6h    restarts 0      cpu 28%   mem 2.0G
  ◉ pod-c    Running       uptime 26m       restarts 1 at 14:06    cpu 62%   mem 2.5G
```

- 每个 pod 一行
- pod 名前的 `◉` 颜色 = 该 pod 的图表配色（pod-a 蓝、pod-b 绿、pod-c 橙）
- `Running` 绿色，其他状态对应警示色
- `uptime` 短的（< 1h）用 WARN 色
- `restarts > 0` 用 WARN 色 + 最近一次时间

---

## 5. 交互与状态机

### 5.1 状态

```rust
pub struct AppState {
    pub args: MonitorArgs,
    pub data: Option<MonitorResponse>,         // 最近一次拉到的数据
    pub last_fetch: Option<Instant>,
    pub fetch_in_flight: bool,
    pub next_refresh_at: Option<Instant>,      // watch 模式

    pub view: View,
    pub focus: FocusState,
    pub watch_paused: bool,                    // 用户按 space 暂停
    pub error: Option<String>,                 // 拉数据失败的提示
    pub terminal_size: (u16, u16),
}

pub enum View {
    Overview,                  // 默认 2×4 总览
    SingleMetric(MetricKind),  // Enter 进入的详情
    TooSmall,                  // 尺寸不够
}

pub struct FocusState {
    /// 总览模式：当前选中的 panel 索引 (0..8)
    pub selected_panel: usize,
}
```

### 5.2 键盘绑定

**全局**：

| 键 | 动作 |
|----|------|
| `q` | 退出 |
| `Ctrl+C` | 退出（SIGINT） |
| `?` | 弹出 help overlay |

**Overview 视图**：

| 键 | 动作 |
|----|------|
| `↑` / `k` | 焦点上移（在 2×4 网格内） |
| `↓` / `j` | 焦点下移 |
| `←` / `h` | 焦点左移 |
| `→` / `l` | 焦点右移 |
| `Tab` | 焦点循环到下一个 panel |
| `Shift+Tab` | 焦点循环到上一个 panel |
| `Enter` | 进入选中 panel 的单图详情 |
| `w` | 切换 watch 模式 on/off |
| `Space` | watch 模式下暂停/继续刷新 |
| `r` | 立即手动刷新 |
| `R` | 切换时间范围（弹出 range picker） |
| `p` | 切换 pod filter（弹出 pod picker） |
| `m` | 切换 metric filter（弹出 metric picker） |
| `a` | **切换聚合方式**：focused panel 在 max/avg/sum/p95/per-pod 之间循环 |
| `u` | **切换流量单位**：QPS / RPM 强制覆盖（默认按中位数自动选） |
| `/` | 进入搜索/过滤模式 |
| `j` | 切换 JSON 输出模式（打印到 stderr 然后继续） |

**SingleMetric 视图**：

| 键 | 动作 |
|----|------|
| `Esc` | 返回 Overview |
| `←` / `→` | 时间轴平移 |
| `+` / `-` | 时间轴缩放 |
| `[` / `]` | 切换到上/下一个 metric |
| `p` | pod filter |
| `c` | 与历史对比（[y] yesterday, [w] last week） |
| `j` | JSON 模式 |

### 5.3 焦点视觉效果

被选中的 panel：边框从 `#3a3a3a`（或状态色）切换为 `#00d4aa`（accent_ok），并将边框样式从 `ROUNDED` 切换为 `DOUBLE` 或 bold 版本。其他 panel 不变。

```
╔═ ◆ Latency  WARN  P99 177ms ════════════╗   ← selected
║                                          ║
║   [曲线]                                  ║
╚══════════════════════════════════════════╝

╭─ ◆ CPU Usage  OK  max 48% ──────────────╮   ← not selected
│                                          │
│   [曲线]                                  │
╰──────────────────────────────────────────╯
```

---

## 6. 数据获取与刷新策略

### 6.1 数据源

```rust
pub trait DataSource: Send + Sync {
    async fn fetch(&self, query: MonitorQuery) -> Result<MonitorResponse>;
}

pub struct MonitorQuery {
    pub app: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub pods: Option<Vec<String>>,
    pub metrics: Option<Vec<MetricKind>>,
}
```

实现：默认是 `HttpDataSource`，从配置文件读 `endpoint` URL。Mock 实现 `MockDataSource` 用于开发和测试。

### 6.2 刷新节奏（三层）

| 层级 | 触发 | 动作 |
|------|------|------|
| 每帧（~30 FPS） | `tokio::time::interval(33ms)` | 重绘倒计时数字、动画状态、用户输入响应 |
| 每秒 | `tokio::time::interval(1s)` | 更新"updated 14:32:08"时间显示 + 倒计时减 1 |
| 每 `interval`（默认 60s）| watch 模式专属 | 后台拉新数据，拉完后触发整图重绘 |
| `SIGWINCH` | 信号 | 清屏 + 重算布局 |

### 6.3 增量重绘

**关键要求**：不能每秒清屏全量重画。`ratatui` 的 `Terminal::draw()` 自带 buffer diff，**只要每次 draw 时传入的 widget 树尽量稳定**，diff 后大部分 cell 不会被发送 ANSI 序列。

实现细节：
- `AppState` 修改用细粒度（修改 next_refresh 不应该改 data）
- ratatui 的渲染是声明式的，但底层 `Buffer::diff` 是 O(W×H)，对 200×60 = 12000 cells 来说毫无压力
- 真实需要关注的是：每次 60s 拉到新数据，曲线整体左移一格，触发 chart 区域**整块**重画。这部分确实没法避免，但只影响 chart cell（不影响 header/footer/边框）

### 6.4 暂停时的处理

`Space` 在 watch 模式下暂停：停止 fetch，但右上角仍显示 `◉ PAUSED`（红色）+ "resumed at 14:35"，倒计时显示 `--:--`。

### 6.5 错误处理

- 网络超时：保留上次成功的数据，在 footer 显示 `⚠ fetch failed (timeout) · last update 12s ago`，颜色 WARN
- HTTP 404 / app 不存在：直接 exit 3 + stderr 报错
- JSON 解析失败：log 到文件 + footer 提示 + 不更新 data

---

## 7. 单图详情模式（`--metric=cpu` 或 Enter 触发）

### 7.1 布局（180×50 参考尺寸）

```
HEADER (1 row)
[ blank ]
KPI ROW: 4 cards × 29 wide, 5 tall
[ blank ]
MAIN: chart_panel (142×26) + sidebar (33×26 with 3 pod cards stacked)
[ blank ]
EVENTS ROW (1 row)
[ blank ]
FOOTER (1 row)
```

### 7.2 KPI 卡片

四个：
1. **CURRENT** — 当前值（多 series 时取 max），副标 "max · pod-c"
2. **3H AVG** — 时间范围内所有 series 的平均
3. **3H PEAK** — 时间范围内最大值，副标 "pod-c · 14:08:42"
4. **TREND 10m** — 最近 10 分钟 vs 30 分钟前的百分比变化，箭头 ↑↓

颜色随 trend：变化 > 5% 用 ALERT，否则 INFO。

### 7.3 主图

- 高 22 行 × 宽 140
- 渐变面积图（参考 § 4.4）
- Y 轴自适应：`y_min = max(0, min - range*0.1)`, `y_max = max + range*0.15`
- X 轴 7 个标签（11:32 → 14:32 七等分）
- 当前时刻标签用 accent 色 + `⬤`

### 7.4 当前值游标（v3 还未实现，必做）

在 chart 最右列（current time 对应的 px column）画一条**垂直虚线**（`┊` 字符或 braille 模拟），延伸整个 chart 高度，颜色 `#3a3a3a`。在每条 series 的当前值位置画一个发光圆点 `●`（用该 series 的 edge_color，加 ANSI bright modifier）。

实现思路：游标层在主图层之后画，单独一遍 pass，覆盖已有 cell 时直接 overwrite。

### 7.5 侧栏 Pod 卡片

每个 pod 一个卡（33 宽 × 10 高），垂直堆叠 3 个。每卡包含：

```
◉ pod-a    41.8%  ↑1.2%
  ▂▃▄▃▂▃▄▅▄▃▂▃▄▃▄▅
  last 26m

 avg  41.9%   p50  41.9%
 min  39.1%   p95  44.6%
 max  44.9%   p99  44.9%
```

- 顶部：pod 名（彩色）+ 当前值（大）+ 趋势箭头
- mini sparkline：最近 26 分钟，用 `▁▂▃▄▅▆▇█`
- 6 个统计值，两列布局

### 7.6 事件行

```
◆ 14:06:00  pod-c restart (OOMKilled, exit 137)    ◆ 14:08:42  CPU spike 51%→87% (pod-c, 18s)    ◆ 13:45:11  deploy v2.4.1 (rolling)
```

不超过 3 个事件，按相关性排序（与当前 metric 相关的优先）。

---

## 8. JSON 输出模式

`--json` 直接打印到 stdout 然后 exit 0，**不进 TUI**。

```json
{
  "app": "demo-app",
  "region": "cn-hangzhou",
  "env": "production",
  "time_range": { "from": "2026-05-18T11:32:00Z", "to": "2026-05-18T14:32:00Z" },
  "resolution_seconds": 60,
  "pods": [
    { "name": "pod-a", "status": "Running", "uptime_seconds": 7286400, "restarts": 0 }
  ],
  "metrics": {
    "cpu": {
      "unit": "%",
      "series": [
        {
          "label": "pod-a",
          "kind": { "Pod": "pod-a" },
          "points": [
            ["2026-05-18T11:32:00Z", 0.42],
            ["2026-05-18T11:33:00Z", 0.41]
          ],
          "stats": { "min": 0.39, "max": 0.45, "avg": 0.42, "p50": 0.42, "p95": 0.44, "p99": 0.45 }
        }
      ]
    }
  },
  "events": [
    { "at": "2026-05-18T14:06:00Z", "kind": "restart", "message": "pod-c OOMKilled" }
  ]
}
```

跟 API 返回的格式几乎一致，只是前端补充计算了 `stats` 字段（min/max/avg/p50/p95/p99）。这样下游 Agent / 脚本可以直接拿来分析。

---

## 9. 配置文件

`~/.config/df/config.toml`（XDG 兼容，跟你 `dnf` 风格一致）：

```toml
[monitor]
default_range = "3h"
default_interval = "60s"
recommended_min_size = [120, 36]

[monitor.endpoint]
url = "http://internal-monitor-api.example.com"
# auth header 从 keychain 读，不放配置文件

[monitor.colors]
# 用户可覆盖默认调色板
# accent_ok = "#00d4aa"
# accent_alert = "#ff6b6b"

[monitor.aliases]
# 应用名别名
"main" = "demo-app-main-cluster"
```

---

## 10. 代码组织建议

```
df/
├── Cargo.toml
├── src/
│   ├── main.rs                       # clap 入口，分发到子命令
│   ├── commands/
│   │   ├── mod.rs
│   │   └── monitor/                  # monitor 子命令独立模块
│   │       ├── mod.rs                # 入口、参数解析、运行
│   │       ├── args.rs               # MonitorArgs 定义
│   │       ├── state.rs              # AppState, View, FocusState
│   │       ├── data/
│   │       │   ├── mod.rs            # DataSource trait
│   │       │   ├── http.rs           # HttpDataSource 实现
│   │       │   ├── mock.rs           # MockDataSource（mock 数据生成器）
│   │       │   └── model.rs          # MonitorResponse 等 deserialize 模型
│   │       ├── layout/
│   │       │   ├── mod.rs            # LayoutTier 枚举 + from_size
│   │       │   ├── overview.rs       # 总览布局算法
│   │       │   ├── single.rs         # 单图布局
│   │       │   └── portrait.rs       # 竖屏布局
│   │       ├── widgets/
│   │       │   ├── mod.rs
│   │       │   ├── chart.rs          # AreaChart / LineChart widget
│   │       │   ├── stacked_bar.rs    # 堆叠柱状
│   │       │   ├── kpi_card.rs       # KPI 卡片
│   │       │   ├── pod_card.rs       # Pod 详情卡
│   │       │   ├── replicas.rs       # Replicas 表格
│   │       │   ├── sparkline.rs      # mini sparkline
│   │       │   ├── header.rs
│   │       │   ├── footer.rs
│   │       │   └── too_small.rs
│   │       ├── render/
│   │       │   ├── mod.rs            # 渲染主入口
│   │       │   ├── braille.rs        # Braille pixel grid utility
│   │       │   └── colors.rs         # Rgb + lerp + 调色板常量
│   │       ├── input.rs              # 键盘事件处理
│   │       ├── json_output.rs        # --json 模式
│   │       └── theme.rs              # 颜色 / 字符集中管理
│   └── lib.rs
└── tests/
    ├── snapshot/                     # 用 insta 对每种布局做 snapshot 测试
    └── data/                         # mock JSON fixtures
```

---

## 11. 实现里程碑

按下列顺序实现，每个里程碑可独立验证：

### M1：MVP CLI（不进 TUI）
- `df monitor <app> --json` 跑通
- `MockDataSource` 生成假数据
- 输出格式符合 § 8
- 验收：`cargo run -- monitor demo-app --json | jq .` 输出合理

### M2：单图渲染（静态）
- 实现 `Braille` 像素网格基础设施
- 实现 `AreaChart` widget（多 series + fade tail + z-order）
- 在最简单的程序里渲染一个 CPU 图，按 q 退出
- 验收：肉眼看曲线分层正确，spike 清晰

### M3：8 panel 总览（默认布局）
- 实现 `TwoByFour` 布局
- 实现 8 个 panel widget
- 静态数据，无刷新、无交互
- 验收：跟本文档 § 4 的设计稿肉眼对比

### M4：键盘交互 + 焦点
- 实现箭头 / hjkl 移动焦点
- 焦点 panel 边框高亮
- Enter 进单图详情，Esc 返回
- q / Ctrl+C 退出
- 验收：能丝滑切换，无渲染 artifact

### M5：桌面响应式布局
- 实现 `LayoutTier::from_size` 中桌面相关档位（SingleColumn/TwoByFour/Large/Sidebar/Portrait）
- 监听 SIGWINCH 重排
- 验收：在 4 种桌面尺寸下手动验证

### M5.5：手机端 Phone 档
- 实现 `Phone` 档单 panel 全屏 + dot indicator
- 上下方向键翻页
- 在 iPhone（Termius/Blink）实际 SSH 测试横竖屏
- SingleMetric 模式的手机降级版（2×2 KPI + 折叠 pod 卡）
- 验收：在 iPhone 横竖屏、iPad mini 上肉眼可用，不出现裁切 / 错位

### M6：单图详情完整版
- KPI 卡片
- 当前值游标
- 侧栏 pod 卡片
- 事件行
- 验收：跟 § 7 的设计稿对比

### M7：watch 模式 + 真实数据
- `--watch` 60s 刷新
- 倒计时显示（每秒更新）
- Space 暂停
- 切换到 `HttpDataSource`
- 验收：在真实环境跑 30 分钟无 leak / 无 ANSI artifact

### M8：JSON 输出 + 配置 + 错误处理
- `--json` 完整实现
- 配置文件加载
- 网络错误、404、解析错误的优雅处理
- 验收：异常路径都有合理表现

---

## 12. 测试策略

1. **单元测试**：`braille.rs`、`colors.rs`、`layout::from_size` 的纯函数
2. **Snapshot 测试**（`insta`）：每种布局 + mock 数据生成 ANSI 输出，跟 git 里的 baseline 对比。Mock 数据用固定 seed，确保 deterministic
3. **集成测试**：用 `expectrl` 或类似工具模拟键盘输入，验证状态机
4. **手动验证**：在真实终端（Ghostty + JetBrainsMono Nerd Font）跑一遍

---

## 13. 已知问题与未来工作

- [ ] 真实终端字符宽度（CJK / emoji）可能跟 Rust 假设不一致，需要 `unicode-width` 处理
- [ ] Windows 终端兼容性未验证（crossterm 支持但 Braille 渲染未必好看）
- [ ] `--compare` 历史对比（[y]esterday / [w]eek ago）M9 再做
- [ ] 多 app 同时监控（`df monitor app-a,app-b` tab 切换）暂不考虑
- [ ] tmux 集成（点击某 panel 可以 popup 详情）暂不考虑
- [ ] **数据 API 的聚合语义需要后端配合**：理想情况下后端在每个 Series 上明示 `aggregation` 字段（max/avg/sum/p99）。如果短期内后端无法升级，前端按 § 2.3 的 fallback 规则猜测并在 subtitle 警告 "aggregation unknown"，提醒用户自行确认
- [ ] **CPU/Memory 的 max-vs-avg 取舍是产品决策**：当前 spec 默认画双线（max 实线 + avg 半透虚线），但如果用户调研后发现大家只看 max 不看 avg，可以改为单线 max + subtitle 备注 avg。这个开关放在配置文件里 `[monitor.cpu] show_avg_line = true`
- [ ] **手机手势**：未来可考虑监听 `iTerm2 mouse protocol` 在 iPad SSH 客户端里支持点击切 panel

---

## 附录 A：与现有工具的差异化

| 工具 | 强项 | df monitor 的差异 |
|------|------|------------------|
| `k9s` | k8s 资源管理 | 我们专注 metrics 不管资源操作 |
| `btop` | 单机系统监控 | 我们是应用级 + 跨多 pod |
| `Grafana` | 完整 dashboard | 我们是 CLI、快、面向 AI Agent |
| `kubectl top` | 简单 | 我们有图、有历史、有事件关联 |

## 附录 B：键盘 cheatsheet（打印到 `?` overlay）

```
                        df monitor — help
            ─────────────────────────────────────────

  Navigation                Time / Filter
    h / ←   focus left        R       change time range
    j / ↓   focus down        p       filter pods
    k / ↑   focus up          m       filter metrics
    l / →   focus right       /       search

  Tab     next panel        Mode
  S-Tab   prev panel          w       toggle watch
  Enter   open detail         space   pause watch (in watch)
  Esc     back to overview    r       refresh now
  q       quit                j       output as JSON
  Ctrl+C  quit (SIGINT)       ?       this help

            ─────────────────────────────────────────
                    press any key to dismiss
```
