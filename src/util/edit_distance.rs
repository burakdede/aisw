/// Levenshtein distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j - 1].min(dp[i - 1][j]).min(dp[i][j - 1])
            };
        }
    }

    dp[m][n]
}

/// Returns the closest match within `max_distance` from `input`, or `None`.
pub fn closest_match<'a>(
    input: &str,
    candidates: &[&'a str],
    max_distance: usize,
) -> Option<&'a str> {
    candidates
        .iter()
        .filter_map(|c| {
            let d = levenshtein(input, c);
            if d <= max_distance {
                Some((d, *c))
            } else {
                None
            }
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn identical_strings() {
        assert_eq!(levenshtein("work", "work"), 0);
    }

    #[test]
    fn single_insertion() {
        assert_eq!(levenshtein("work", "works"), 1);
    }

    #[test]
    fn single_deletion() {
        assert_eq!(levenshtein("works", "work"), 1);
    }

    #[test]
    fn single_substitution() {
        assert_eq!(levenshtein("work", "worm"), 1);
    }

    #[test]
    fn closest_match_finds_within_distance() {
        assert_eq!(closest_match("wrk", &["work", "personal"], 2), Some("work"));
    }

    #[test]
    fn closest_match_returns_none_when_too_far() {
        assert_eq!(closest_match("xyz123", &["work", "personal"], 2), None);
    }
}
