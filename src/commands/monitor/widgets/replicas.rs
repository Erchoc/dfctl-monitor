use crate::commands::monitor::data::PodInfo;
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::util::format_duration_short;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub struct ReplicasTable<'a> {
    pub pods: &'a [PodInfo],
}

impl<'a> Widget for ReplicasTable<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 {
            return;
        }
        let mut y = area.y;

        // header
        let head = Line::from(vec![
            Span::styled(
                format!("  {:<8} ", "POD"),
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10} ", "STATUS"),
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10} ", "UPTIME"),
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10} ", "RESTART"),
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<8} ", "CPU"),
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<6}", "MEM"),
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            ),
        ]);
        Paragraph::new(head).render(Rect::new(area.x, y, area.width, 1), buf);
        y += 1;
        if y >= area.y + area.height {
            return;
        }
        // separator dim line
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char('─');
                cell.set_style(Style::default().fg(BORDER_GRID.to_color()));
            }
        }
        y += 1;

        for pod in self.pods {
            if y >= area.y + area.height {
                break;
            }
            let pod_c = pod_color(&pod.name);
            let status_color = if pod.status == "Running" {
                ACCENT_OK
            } else {
                ACCENT_ALERT
            };
            let uptime_color = if pod.uptime_seconds < 3600 {
                ACCENT_WARN
            } else {
                TEXT_PRIMARY
            };
            let restart_str = if pod.restarts == 0 {
                "0".to_string()
            } else {
                format!("{} ({} ago)", pod.restarts, super::super::util::ago(pod.last_restart_at.unwrap()))
            };
            let restart_color = if pod.restarts > 0 {
                ACCENT_WARN
            } else {
                TEXT_PRIMARY
            };
            let line = Line::from(vec![
                Span::styled("◉ ", Style::default().fg(pod_c.to_color())),
                Span::styled(
                    format!("{:<8} ", pod.name),
                    Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<10} ", pod.status),
                    Style::default().fg(status_color.to_color()),
                ),
                Span::styled(
                    format!("{:<10} ", format_duration_short(pod.uptime_seconds)),
                    Style::default().fg(uptime_color.to_color()),
                ),
                Span::styled(
                    format!("{:<10} ", restart_str),
                    Style::default().fg(restart_color.to_color()),
                ),
                Span::styled(
                    format!("{:<7} ", format!("{:.1}%", pod.cpu_pct)),
                    Style::default().fg(TEXT_PRIMARY.to_color()),
                ),
                Span::styled(
                    format!("{:.1}G", pod.mem_bytes as f64 / 1024.0 / 1024.0 / 1024.0),
                    Style::default().fg(TEXT_PRIMARY.to_color()),
                ),
            ]);
            Paragraph::new(line).render(Rect::new(area.x, y, area.width, 1), buf);
            y += 1;
        }
    }
}
