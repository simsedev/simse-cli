//! Levenshtein distance for command suggestion.

/// Compute Levenshtein edit distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    // Cap input length: called per autocomplete keystroke, allocates an
    // (a+1)×(b+1) matrix. A long paste would otherwise allocate gigabytes;
    // command suggestion needs nothing beyond a couple hundred chars.
    const MAX_LEN: usize = 256;
    let a: Vec<char> = a.chars().take(MAX_LEN).collect();
    let b: Vec<char> = b.chars().take(MAX_LEN).collect();
    let a_len = a.len();
    let b_len = b.len();

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(a_len + 1) {
        row[0] = i;
    }
    for (j, val) in matrix[0].iter_mut().enumerate().take(b_len + 1) {
        *val = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i - 1][j - 1] + cost)
                .min(matrix[i][j - 1] + 1);
        }
    }
    matrix[a_len][b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings() {
        assert_eq!(levenshtein("search", "search"), 0);
    }

    #[test]
    fn one_edit() {
        assert_eq!(levenshtein("search", "sarch"), 1);
    }

    #[test]
    fn two_edits() {
        assert_eq!(levenshtein("search", "serh"), 2);
    }

    #[test]
    fn empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn completely_different() {
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }
}
