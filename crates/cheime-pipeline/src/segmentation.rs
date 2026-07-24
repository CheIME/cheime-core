use crate::CodeSegment;

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

#[derive(Clone, Debug, Eq, PartialEq)]
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
}

impl SegmentationGraph {
    pub fn new(input_len: usize) -> Self {
        Self {
            input_len,
            outgoing: vec![Vec::new(); input_len + 1],
        }
    }

    pub fn add_edge(&mut self, edge: SyllableEdge) {
        assert!(edge.span.start < edge.span.end);
        assert!(edge.span.end <= self.input_len);
        self.outgoing[edge.span.start].push(edge);
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
    /// decoder consumes the graph directly. It follows the longest complete
    /// edge, then the longest incomplete edge, then raw input.
    pub fn primary_path(&self) -> Vec<CodeSegment> {
        let mut result = Vec::new();
        let mut offset = 0;
        while offset < self.input_len {
            let Some(edge) = self
                .edges_from(offset)
                .iter()
                .filter(|edge| edge.kind == SyllableKind::Complete)
                .max_by_key(|edge| edge.span.end)
                .or_else(|| {
                    self.edges_from(offset)
                        .iter()
                        .filter(|edge| edge.kind == SyllableKind::Incomplete)
                        .max_by_key(|edge| edge.span.end)
                })
                .or_else(|| self.edges_from(offset).first())
            else {
                break;
            };
            result.push(CodeSegment {
                code: edge.canonical.clone(),
                tag: match edge.kind {
                    SyllableKind::Raw => "raw",
                    SyllableKind::Complete => "pinyin",
                    SyllableKind::Incomplete => "pinyin-incomplete",
                }
                .to_owned(),
            });
            offset = edge.span.end;
        }
        result
    }
}
