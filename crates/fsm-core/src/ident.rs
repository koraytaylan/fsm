//! Identifier helpers: Levenshtein suggestions.

/// Best candidate at Levenshtein distance ≤ 2. Ties keep the first in iteration order.
pub fn suggest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut best: Option<(&'a str, usize)> = None;
    for c in candidates {
        let d = levenshtein(name, c);
        match best {
            None if d <= 2 => best = Some((c, d)),
            Some((_, bd)) if d < bd && d <= 2 => best = Some((c, d)),
            _ => {}
        }
    }
    best.map(|(c, _)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_one() {
        assert_eq!(suggest("limt", ["limit"]), Some("limit"));
        assert_eq!(suggest("amonut", ["amount"]), Some("amount"));
    }

    #[test]
    fn distance_three_is_none() {
        assert_eq!(suggest("abc", ["xyz"]), None);
    }

    #[test]
    fn tie_keeps_first() {
        assert_eq!(suggest("ab", ["aa", "ac", "zz"]), Some("aa"));
    }
}
