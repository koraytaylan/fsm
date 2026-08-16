//! NIST byte-oriented SHA-256 vectors and incremental agreement.

use fsm_core::sha256::{Sha256, from_hex, sha256, to_hex};

struct Vector {
    kind: String,
    payload: String,
    digest: String,
}

fn load_vectors() -> Vec<Vector> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/sha256/vectors.txt"
    ))
    .unwrap();
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        out.push(Vector {
            kind: parts.next().unwrap().into(),
            payload: parts.next().unwrap().into(),
            digest: parts.next().unwrap().into(),
        });
    }
    out
}

fn message(v: &Vector) -> Vec<u8> {
    match v.kind.as_str() {
        "empty" => Vec::new(),
        "ascii" => v.payload.as_bytes().to_vec(),
        "repeat" => {
            let (ch, n) = v.payload.split_once('*').unwrap();
            let n: usize = n.parse().unwrap();
            ch.as_bytes()[0].repeat_vec(n)
        }
        "hex" => from_hex(&v.payload).expect("hex payload"),
        other => panic!("unknown kind {other}"),
    }
}

trait RepeatExt {
    fn repeat_vec(self, n: usize) -> Vec<u8>;
}

impl RepeatExt for u8 {
    fn repeat_vec(self, n: usize) -> Vec<u8> {
        vec![self; n]
    }
}

fn digest_chunks(msg: &[u8], chunk: usize) -> [u8; 32] {
    let mut h = Sha256::new();
    if chunk == 0 {
        h.update(msg);
    } else {
        for part in msg.chunks(chunk) {
            h.update(part);
        }
    }
    h.finalize()
}

#[test]
fn nist_vectors_one_shot_and_chunks() {
    let chunks = [1usize, 3, 64, 65, 4096];
    for v in load_vectors() {
        let msg = message(&v);
        let want = from_hex(&v.digest).expect("digest hex");
        let one = sha256(&msg);
        assert_eq!(to_hex(&one), v.digest, "one-shot {}", v.payload);
        assert_eq!(one.as_slice(), want.as_slice());
        for c in chunks {
            let got = digest_chunks(&msg, c);
            assert_eq!(to_hex(&got), v.digest, "chunk {c} {}", v.payload);
        }
    }
}

#[test]
fn incremental_seeded_10kib() {
    let mut buf = vec![0u8; 10 * 1024];
    let mut state = 0xA5A5_A5A5u64;
    for b in &mut buf {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *b = state as u8;
    }
    let one = sha256(&buf);
    for c in [1usize, 3, 64, 65, 4096] {
        assert_eq!(digest_chunks(&buf, c), one, "chunk {c}");
    }
}
