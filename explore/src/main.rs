use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, env, fs, io, io::prelude::*};
use strata::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct InputDocument {
    text: String,
    layers: HashMap<String, Vec<ValidExtent>>,
}

fn read_document(filename: &str) -> InputDocument {
    let s = fs::read_to_string(filename).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn index_document(content: &str) -> HashMap<String, Vec<ValidExtent>> {
    let mut index = HashMap::new();
    let mut chars = content.char_indices();

    loop {
        for _ in chars.by_ref().take_while_ref(|&(_, c)| !c.is_alphabetic()) {}

        let (first, last) = {
            let mut words = chars.by_ref().take_while_ref(|&(_, c)| c.is_alphabetic());
            (words.next(), words.last())
        };

        let extent = match (first, last) {
            (Some(s), Some(e)) => (s.0 as u64, e.0 as u64 + 1),
            (Some(s), None) => (s.0 as u64, s.0 as u64 + 1),
            (None, _) => break,
        };

        let word = content[(extent.0 as usize)..(extent.1 as usize)].to_lowercase();

        index
            .entry(word)
            .or_insert_with(Vec::new)
            .push((extent.0, extent.1));
    }

    index
}

fn json_to_query<'a>(
    json: &Value,
    index: &'a HashMap<String, Vec<ValidExtent>>,
    layers: &'a HashMap<String, Vec<ValidExtent>>,
) -> Result<Box<Algebra + 'a>, &'static str> {
    let op: Box<Algebra> = match *json {
        Value::String(ref s) => Box::new(index.get(s).map(Vec::as_slice).unwrap_or(&[])),
        Value::Array(ref a) => {
            let cmd = a.get(0);
            let lhs = a.get(1);
            let rhs = a.get(2);

            let (cmd, lhs, rhs) = match (cmd, lhs, rhs) {
                (Some(a), Some(b), Some(c)) => (a, b, c),
                _ => return Err("Malformed op"),
            };

            let cmd = match *cmd {
                Value::String(ref s) => s,
                _ => return Err("Not a valid op"),
            };

            if cmd == "L" {
                if let Value::String(ref s) = *lhs {
                    let z = layers.get(s).map(Vec::as_slice).unwrap_or(&[]);
                    return Ok(Box::new(z));
                } else {
                    return Err("bad layer request");
                }
            }

            let a = json_to_query(lhs, index, layers)?;
            let b = json_to_query(rhs, index, layers)?;

            match cmd.as_str() {
                "<" => Box::new(ContainedIn::new(a, b)),
                ">" => Box::new(Containing::new(a, b)),
                "/<" => Box::new(NotContainedIn::new(a, b)),
                "/>" => Box::new(NotContaining::new(a, b)),
                "&" => Box::new(BothOf::new(a, b)),
                "|" => Box::new(OneOf::new(a, b)),
                "->" => Box::new(FollowedBy::new(a, b)),
                _ => return Err("Unknown op"),
            }
        }
        _ => Box::new(Empty),
    };

    Ok(op)
}

struct Index {
    data: Vec<String>,
    index: HashMap<String, Vec<ValidExtent>>,
    layers: HashMap<String, Vec<ValidExtent>>,
}

fn index() -> Index {
    let mut data = Vec::new();
    let mut index = HashMap::new();
    let mut layers = HashMap::new();

    for file in env::args().skip(1) {
        let doc = read_document(&file);
        let doc_index = index_document(&doc.text);

        data.push(doc.text);
        for (word, extents) in doc_index {
            index.entry(word).or_insert_with(Vec::new).extend(extents);
        }
        for (name, extents) in doc.layers {
            layers.entry(name).or_insert_with(Vec::new).extend(extents);
        }
    }

    println!("=Index=");
    for (word, extents) in &index {
        println!("{}: {:?}", word, extents);
    }
    println!("=Layers=");
    for (layer, extents) in &layers {
        println!("{}: {:?}", layer, extents);
    }

    Index {
        data,
        index,
        layers,
    }
}

fn query_stdin(
    data: Vec<String>,
    index: HashMap<String, Vec<ValidExtent>>,
    layers: HashMap<String, Vec<ValidExtent>>,
) {
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let l = line.unwrap();

        let q = match serde_json::from_str(&l) {
            Ok(q) => q,
            Err(e) => {
                println!("Error: {}", e);
                continue;
            }
        };

        let op = match json_to_query(&q, &index, &layers) {
            Ok(op) => op,
            Err(e) => {
                println!("Error: {}", e);
                continue;
            }
        };

        for extent in op.iter_tau() {
            let ex = (extent.0, extent.1);
            let content = &data[0]; // HACK: 0 isnt right
            println!("{:?}: {}", ex, &content[(ex.0 as usize)..(ex.1 as usize)]);
        }
    }
}

fn main() {
    let index = index();
    query_stdin(index.data, index.index, index.layers);
}
