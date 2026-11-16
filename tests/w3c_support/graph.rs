//! Capped fixture adapter, not a production database or alternate reasoner.
use inference_engine::{
    entailment::EntailmentTerms,
    reasoning::{TripleCursor, TripleSource},
};
use oxrdf::{BlankNode, Term, Triple};
use std::{collections::HashMap, io, sync::Arc};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Key {
    Atomic(Term),
    Blank(u8, BlankNode),
    Triple([u64; 3]),
}
#[derive(Default)]
pub struct Terms {
    nodes: Vec<Key>,
    index: HashMap<Key, u64>,
}
impl Terms {
    fn intern(&mut self, term: Term, scope: u8, depth: usize) -> u64 {
        assert!(depth < 64, "bounded fixture nesting exhausted");
        let key = match term {
            Term::BlankNode(blank) => Key::Blank(scope, blank),
            Term::Triple(triple) => Key::Triple(self.triple(*triple, scope, depth + 1)),
            atomic => Key::Atomic(atomic),
        };
        if let Some(id) = self.index.get(&key) {
            return *id;
        }
        // bounded: fixtures enforce at most 4096 exact term keys per source/target pair.
        assert!(
            self.nodes.len() < 4096,
            "bounded fixture dictionary exhausted"
        );
        let id = self.nodes.len() as u64;
        self.index.insert(key.clone(), id);
        self.nodes.push(key);
        id
    }
    fn triple(&mut self, triple: Triple, scope: u8, depth: usize) -> [u64; 3] {
        [
            self.intern(triple.subject.into(), scope, depth),
            self.intern(triple.predicate.into(), scope, depth),
            self.intern(triple.object, scope, depth),
        ]
    }
    pub fn graph(&mut self, triples: Vec<Triple>, scope: u8) -> Graph {
        assert!(triples.len() <= 1024);
        // bounded: at most 1024 parsed fixture rows; keys reference the capped fixture dictionary.
        let mut rows = Vec::new();
        for triple in triples {
            rows.push(self.triple(triple, scope, 0));
        }
        Graph(Arc::new(rows))
    }
    fn key(&self, term: &u64) -> io::Result<&Key> {
        self.nodes
            .get(*term as usize)
            .ok_or_else(|| io::Error::other("unknown fixture term"))
    }
}
impl EntailmentTerms for Terms {
    type Term = u64;
    type Error = io::Error;
    async fn is_blank(&self, term: &u64) -> io::Result<bool> {
        Ok(matches!(self.key(term)?, Key::Blank(..)))
    }
    async fn triple_term(&self, term: &u64) -> io::Result<Option<[u64; 3]>> {
        Ok(match self.key(term)? {
            Key::Triple(triple) => Some(*triple),
            _ => None,
        })
    }
    fn is_cancelled(&self) -> bool {
        false
    }
}
pub struct Graph(Arc<Vec<[u64; 3]>>);
pub struct Cursor {
    graph: Arc<Vec<[u64; 3]>>,
    pattern: [Option<u64>; 3],
    position: usize,
}
impl TripleSource for Graph {
    type Term = u64;
    type Error = io::Error;
    type Cursor = Cursor;
    async fn scan(&self, pattern: [Option<u64>; 3]) -> io::Result<Cursor> {
        Ok(Cursor {
            graph: self.0.clone(),
            pattern,
            position: 0,
        })
    }
}
impl TripleCursor for Cursor {
    type Term = u64;
    type Error = io::Error;
    async fn next(&mut self) -> io::Result<Option<[u64; 3]>> {
        while let Some(row) = self.graph.get(self.position) {
            self.position += 1;
            if self
                .pattern
                .iter()
                .zip(row)
                .all(|(p, t)| p.is_none_or(|p| p == *t))
            {
                return Ok(Some(*row));
            }
        }
        Ok(None)
    }
}
