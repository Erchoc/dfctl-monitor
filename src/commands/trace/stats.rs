//! Derived trace statistics: per-span self-time & depth, the critical path,
//! per-service breakdown, the bottleneck span, and a one-line summary.
//!
//! All of this is computed frontend-side from the raw `TraceResponse` so the
//! backend only has to ship spans.

use super::data::{Span, TraceResponse};
use super::summary::{ServiceBreak, SummaryInput, SummarySource};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct TraceStats {
    /// span_id → self time (duration minus child-covered time), µs.
    pub self_us: HashMap<String, u64>,
    /// span_id → depth (root = 0).
    pub depth: HashMap<String, u16>,
    /// parent span_id → child span_ids, sorted by start offset.
    pub children: HashMap<String, Vec<String>>,
    /// root span ids (usually one), sorted by start.
    pub roots: Vec<String>,
    /// Critical path span ids, ordered root → leaf.
    pub critical_path: Vec<String>,
    pub critical_set: HashSet<String>,
    /// Per-service self-time, sorted descending.
    pub breakdown: Vec<ServiceBreak>,
    /// span_id of the highest self-time span.
    pub bottleneck: Option<String>,
    /// error span ids, ordered by start offset.
    pub error_spans: Vec<String>,
    pub summary: String,
}

impl TraceStats {
    pub fn compute(trace: &TraceResponse, summarizer: &dyn SummarySource) -> Self {
        let by_id: HashMap<&str, &Span> =
            trace.spans.iter().map(|s| (s.span_id.as_str(), s)).collect();

        // children map + roots
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut roots: Vec<String> = Vec::new();
        for s in &trace.spans {
            match &s.parent_id {
                Some(p) if by_id.contains_key(p.as_str()) => {
                    children.entry(p.clone()).or_default().push(s.span_id.clone());
                }
                _ => roots.push(s.span_id.clone()),
            }
        }
        let start_of = |id: &str| by_id.get(id).map(|s| s.start_offset_us).unwrap_or(0);
        for v in children.values_mut() {
            v.sort_by_key(|id| start_of(id));
        }
        roots.sort_by_key(|id| start_of(id));

        // depth (BFS from roots)
        let mut depth: HashMap<String, u16> = HashMap::new();
        let mut stack: Vec<(String, u16)> = roots.iter().map(|r| (r.clone(), 0)).collect();
        while let Some((id, d)) = stack.pop() {
            depth.insert(id.clone(), d);
            if let Some(kids) = children.get(&id) {
                for k in kids {
                    stack.push((k.clone(), d + 1));
                }
            }
        }

        // self-time: duration minus union of child intervals (clipped to span window)
        let mut self_us: HashMap<String, u64> = HashMap::new();
        for s in &trace.spans {
            let covered = children
                .get(&s.span_id)
                .map(|kids| {
                    let intervals: Vec<(u64, u64)> = kids
                        .iter()
                        .filter_map(|k| by_id.get(k.as_str()))
                        .map(|c| {
                            let a = c.start_offset_us.max(s.start_offset_us);
                            let b = c.end_offset_us().min(s.end_offset_us());
                            (a, b.max(a))
                        })
                        .collect();
                    union_len(&intervals)
                })
                .unwrap_or(0);
            self_us.insert(s.span_id.clone(), s.duration_us.saturating_sub(covered));
        }

        // critical path: from the latest-ending root, always follow the
        // latest-ending child.
        let mut critical_path: Vec<String> = Vec::new();
        if let Some(root) = roots
            .iter()
            .max_by_key(|id| by_id.get(id.as_str()).map(|s| s.end_offset_us()).unwrap_or(0))
        {
            let mut cur = root.clone();
            loop {
                critical_path.push(cur.clone());
                let next = children.get(&cur).and_then(|kids| {
                    kids.iter()
                        .max_by_key(|k| by_id.get(k.as_str()).map(|s| s.end_offset_us()).unwrap_or(0))
                        .cloned()
                });
                match next {
                    Some(n) => cur = n,
                    None => break,
                }
            }
        }
        let critical_set: HashSet<String> = critical_path.iter().cloned().collect();

        // per-service self-time breakdown
        let total: u64 = trace.duration_us.max(1);
        let mut svc: HashMap<String, u64> = HashMap::new();
        for s in &trace.spans {
            *svc.entry(s.service.clone()).or_default() += self_us[&s.span_id];
        }
        let mut breakdown: Vec<ServiceBreak> = svc
            .into_iter()
            .map(|(service, self_us)| ServiceBreak {
                service,
                self_us,
                pct: self_us as f64 / total as f64,
            })
            .collect();
        breakdown.sort_by(|a, b| b.self_us.cmp(&a.self_us));

        // bottleneck = span with max self-time
        let bottleneck = trace
            .spans
            .iter()
            .max_by_key(|s| self_us[&s.span_id])
            .map(|s| s.span_id.clone());

        // error spans by start
        let mut error_spans: Vec<String> = trace
            .spans
            .iter()
            .filter(|s| s.is_error())
            .map(|s| s.span_id.clone())
            .collect();
        error_spans.sort_by_key(|id| start_of(id));

        // summary
        let bottleneck_span = bottleneck.as_ref().and_then(|id| by_id.get(id.as_str()).copied());
        let err_refs: Vec<&Span> = error_spans
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .collect();
        let summary = summarizer.summarize(&SummaryInput {
            trace,
            breakdown: &breakdown,
            bottleneck: bottleneck_span,
            critical_path: &critical_path,
            error_spans: &err_refs,
        });

        Self {
            self_us,
            depth,
            children,
            roots,
            critical_path,
            critical_set,
            breakdown,
            bottleneck,
            error_spans,
            summary,
        }
    }

    /// Spans in depth-first visible order, honoring the collapsed set.
    /// Returns `(span_id, depth, is_last_child)`.
    pub fn visible_order(&self, collapsed: &HashSet<String>) -> Vec<(String, u16, bool)> {
        let mut out = Vec::new();
        // iterate roots
        let roots = self.roots.clone();
        let n = roots.len();
        for (i, r) in roots.iter().enumerate() {
            self.walk(r, collapsed, i + 1 == n, &mut out);
        }
        out
    }

    fn walk(
        &self,
        id: &str,
        collapsed: &HashSet<String>,
        is_last: bool,
        out: &mut Vec<(String, u16, bool)>,
    ) {
        let d = *self.depth.get(id).unwrap_or(&0);
        out.push((id.to_string(), d, is_last));
        if collapsed.contains(id) {
            return;
        }
        if let Some(kids) = self.children.get(id) {
            let n = kids.len();
            for (i, k) in kids.iter().enumerate() {
                self.walk(k, collapsed, i + 1 == n, out);
            }
        }
    }

    pub fn has_children(&self, id: &str) -> bool {
        self.children.get(id).map(|k| !k.is_empty()).unwrap_or(false)
    }
}

/// Total length covered by a set of [start,end) intervals (union).
fn union_len(intervals: &[(u64, u64)]) -> u64 {
    if intervals.is_empty() {
        return 0;
    }
    let mut iv: Vec<(u64, u64)> = intervals.to_vec();
    iv.sort_by_key(|x| x.0);
    let mut total = 0u64;
    let mut cur_start = iv[0].0;
    let mut cur_end = iv[0].1;
    for &(s, e) in iv.iter().skip(1) {
        if s > cur_end {
            total += cur_end - cur_start;
            cur_start = s;
            cur_end = e;
        } else {
            cur_end = cur_end.max(e);
        }
    }
    total += cur_end - cur_start;
    total
}
