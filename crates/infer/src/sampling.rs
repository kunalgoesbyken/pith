pub fn log_sum_exp(logits: &[f32]) -> f32 {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum: f32 = logits.iter().map(|l| (l - max).exp()).sum();
    max + sum.ln()
}

fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

pub fn consistent_with_prefix(piece: &[u8], remaining: &[u8]) -> bool {
    if piece.is_empty() {
        return false;
    }
    if piece.starts_with(remaining) {
        return true;
    }
    remaining.starts_with(piece)
        && remaining
            .get(piece.len())
            .is_some_and(|byte| !is_utf8_continuation(*byte))
}

pub fn valid_utf8_prefix(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap_or_default(),
    }
}

fn common_prefix_len<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

pub fn reusable_prefix<T: PartialEq>(cached: &[T], tokens: &[T]) -> usize {
    common_prefix_len(cached, tokens).min(tokens.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_consistency() {
        let tsu = "つい".as_bytes();
        let enye = "ñ".as_bytes();
        let enye_a = "ña".as_bytes();
        let rows: [(&[u8], &[u8], bool); 12] = [
            (b" contract", b" contr", true),
            (b" con", b" contr", true),
            (b" cat", b" contr", false),
            (b"", b" contr", false),
            (b"x", b"", true),
            (&tsu[..3], tsu, true),
            (tsu, &tsu[..3], true),
            (&tsu[..1], tsu, false),
            (&tsu[..2], tsu, false),
            (&tsu[..4], tsu, false),
            (enye, enye_a, true),
            (&enye[..1], enye_a, false),
        ];
        for row in rows {
            let (piece, remaining, expected) = row;
            assert_eq!(
                consistent_with_prefix(piece, remaining),
                expected,
                "{row:?}"
            );
        }
    }

    #[test]
    fn utf8_prefix_drops_incomplete_tail() {
        let wave = "hi 👋".as_bytes();
        assert_eq!(valid_utf8_prefix(&wave[..wave.len() - 1]), "hi ");
        assert_eq!(valid_utf8_prefix(wave), "hi 👋");
    }

    #[test]
    fn reusable_prefix_keeps_the_common_prefix_but_re_evaluates_the_last_token() {
        let rows: [(&[i32], &[i32], usize); 9] = [
            (&[1, 2, 3], &[1, 2, 3, 4, 5], 3),
            (&[1, 2, 3, 4], &[1, 2, 9, 4], 2),
            (&[1, 2, 3, 4], &[1, 2], 1),
            (&[1, 2, 3], &[1, 2, 3], 2),
            (&[1, 2, 3, 4], &[1, 2, 3], 2),
            (&[], &[1, 2], 0),
            (&[7, 8], &[1, 2], 0),
            (&[1], &[], 0),
            (&[1, 2], &[1, 2, 3], 2),
        ];
        for row in rows {
            let (cached, tokens, expected) = row;
            assert_eq!(reusable_prefix(cached, tokens), expected, "{row:?}");
        }
    }
}
