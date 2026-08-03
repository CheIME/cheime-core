use crate::CodeSegment;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InputSpan {
    pub start: usize,
    pub end: usize,
}

impl InputSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SyllableKind {
    Complete,
    Incomplete,
    Raw,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyllableEdge {
    pub span: InputSpan,
    pub raw: String,
    pub canonical: String,
    pub kind: SyllableKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentationGraph {
    input_len: usize,
    outgoing: Vec<Vec<SyllableEdge>>,
    edge_costs: HashMap<SyllableEdge, i64>,
}

#[derive(Clone)]
struct PrimaryPath {
    raw_edges: usize,
    incomplete_edges: usize,
    singleton_complete_edges: usize,
    segments: Vec<CodeSegment>,
}

impl PrimaryPath {
    fn terminal() -> Self {
        Self {
            raw_edges: 0,
            incomplete_edges: 0,
            singleton_complete_edges: 0,
            segments: Vec::new(),
        }
    }

    fn prepend(edge: &SyllableEdge, suffix: &Self) -> Self {
        let mut segments = Vec::with_capacity(suffix.segments.len() + 1);
        segments.push(CodeSegment {
            code: edge.canonical.clone(),
            tag: match edge.kind {
                SyllableKind::Raw => "raw",
                SyllableKind::Complete => "pinyin",
                SyllableKind::Incomplete => "pinyin-incomplete",
            }
            .to_owned(),
        });
        segments.extend(suffix.segments.clone());
        Self {
            raw_edges: suffix.raw_edges + usize::from(edge.kind == SyllableKind::Raw),
            incomplete_edges: suffix.incomplete_edges
                + usize::from(edge.kind == SyllableKind::Incomplete),
            singleton_complete_edges: suffix.singleton_complete_edges
                + usize::from(edge.kind == SyllableKind::Complete && edge.raw.len() == 1),
            segments,
        }
    }

    fn is_preferred_to(&self, other: &Self) -> bool {
        (
            self.raw_edges,
            self.incomplete_edges,
            self.singleton_complete_edges,
            self.segments.len(),
        ) < (
            other.raw_edges,
            other.incomplete_edges,
            other.singleton_complete_edges,
            other.segments.len(),
        )
    }
}

impl SegmentationGraph {
    pub fn new(input_len: usize) -> Self {
        Self {
            input_len,
            outgoing: vec![Vec::new(); input_len + 1],
            edge_costs: HashMap::new(),
        }
    }

    pub fn add_edge(&mut self, edge: SyllableEdge) {
        self.add_edge_with_cost(edge, 0);
    }

    /// Adds an edge with a non-negative fixed-point input-model cost.
    ///
    /// Re-adding an identical edge keeps the lowest cost. Negative values are
    /// clamped to zero so correction rules cannot accidentally become bonuses.
    pub fn add_edge_with_cost(&mut self, edge: SyllableEdge, cost: i64) {
        assert!(edge.span.start < edge.span.end);
        assert!(edge.span.end <= self.input_len);
        let cost = cost.max(0);
        if cost == 0 {
            self.edge_costs.remove(&edge);
        } else {
            self.edge_costs
                .entry(edge.clone())
                .and_modify(|existing| *existing = (*existing).min(cost))
                .or_insert(cost);
        }
        self.outgoing[edge.span.start].push(edge);
    }

    pub fn edge_cost(&self, edge: &SyllableEdge) -> i64 {
        self.edge_costs.get(edge).copied().unwrap_or(0)
    }

    pub fn input_len(&self) -> usize {
        self.input_len
    }

    pub fn is_empty(&self) -> bool {
        self.input_len == 0
    }

    pub fn edges_from(&self, offset: usize) -> &[SyllableEdge] {
        self.outgoing.get(offset).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn edges(&self) -> impl Iterator<Item = &SyllableEdge> {
        self.outgoing.iter().flatten()
    }

    pub fn has_costs(&self) -> bool {
        !self.edge_costs.is_empty()
    }

    pub fn has_complete_path(&self) -> bool {
        let mut reachable = vec![false; self.input_len + 1];
        reachable[0] = true;
        for offset in 0..self.input_len {
            if !reachable[offset] {
                continue;
            }
            for edge in self.edges_from(offset) {
                if edge.kind == SyllableKind::Complete
                    || (edge.kind == SyllableKind::Raw && edge.raw == "'")
                {
                    reachable[edge.span.end] = true;
                }
            }
        }
        reachable[self.input_len]
    }

    pub fn finish(&mut self) {
        for edges in &mut self.outgoing {
            edges.sort_by(|left, right| {
                (
                    left.span.end,
                    left.canonical.as_str(),
                    left.kind,
                    left.raw.as_str(),
                )
                    .cmp(&(
                        right.span.end,
                        right.canonical.as_str(),
                        right.kind,
                        right.raw.as_str(),
                    ))
            });
            edges.dedup_by(|left, right| {
                left.span == right.span
                    && left.canonical == right.canonical
                    && left.kind == right.kind
            });
        }
    }

    /// Transitional linear view used by translators until the word-graph
    /// decoder consumes the graph directly. It chooses a deterministic path
    /// across the whole graph rather than greedily taking a local longest edge.
    pub fn primary_path(&self) -> Vec<CodeSegment> {
        let mut paths = vec![None; self.input_len + 1];
        paths[self.input_len] = Some(PrimaryPath::terminal());

        for offset in (0..self.input_len).rev() {
            for edge in self.edges_from(offset) {
                let Some(suffix) = &paths[edge.span.end] else {
                    continue;
                };
                let candidate = PrimaryPath::prepend(edge, suffix);
                let replace = paths[offset]
                    .as_ref()
                    .is_none_or(|current| candidate.is_preferred_to(current));
                if replace {
                    paths[offset] = Some(candidate);
                }
            }
        }

        paths
            .into_iter()
            .next()
            .flatten()
            .map(|path| path.segments)
            .unwrap_or_default()
    }
}
