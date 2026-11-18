pub fn log_sum_exp(logits: &[f32]) -> f32 {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum: f32 = logits.iter().map(|l| (l - max).exp()).sum();
    max + sum.ln()
}

pub fn consistent_with_prefix(piece: &[u8], remaining: &[u8]) -> bool {
    !piece.is_empty() && (piece.starts_with(remaining) || remaining.starts_with(piece))
}

pub fn valid_utf8_prefix(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap_or_default(),
    }
}

pub fn common_prefix_len<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_consistency() {
        assert!(consistent_with_prefix(b" contract", b" contr"));
        assert!(consistent_with_prefix(b" con", b" contr"));
        assert!(!consistent_with_prefix(b" cat", b" contr"));
        assert!(!consistent_with_prefix(b"", b" contr"));
        assert!(consistent_with_prefix(b"x", b""));
    }

    #[test]
    fn utf8_prefix_drops_incomplete_tail() {
        let wave = "hi 👋".as_bytes();
        assert_eq!(valid_utf8_prefix(&wave[..wave.len() - 1]), "hi ");
        assert_eq!(valid_utf8_prefix(wave), "hi 👋");
    }

    #[test]
    fn lse_matches_naive() {
        let logits = [1.0f32, 2.0, 3.0];
        let naive = logits.iter().map(|l| l.exp()).sum::<f32>().ln();
        assert!((log_sum_exp(&logits) - naive).abs() < 1e-5);
    }

    #[test]
    fn common_prefix() {
        assert_eq!(common_prefix_len(&[1, 2, 3], &[1, 2, 4]), 2);
        assert_eq!(common_prefix_len::<i32>(&[], &[1]), 0);
    }
}
