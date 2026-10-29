/// Exactly 1 takes `one`, everything else `many`.
pub(super) fn two(n: u64, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 {
        one
    } else {
        many
    }
}

/// 0 and 1 take the singular (French).
pub(super) fn zero_or_one(n: u64, one: &'static str, many: &'static str) -> &'static str {
    if n <= 1 {
        one
    } else {
        many
    }
}

/// Slavic triad (be/pl/ru/uk): 1/21/31 take `one`, 2–4/22–24 take `few`,
/// everything else (0, 5–20, 11–14 and their tens) takes `many`.
pub(super) fn slavic(
    n: u64,
    one: &'static str,
    few: &'static str,
    many: &'static str,
) -> &'static str {
    let (m10, m100) = (n % 10, n % 100);
    if m10 == 1 && m100 != 11 {
        one
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        few
    } else {
        many
    }
}

/// Slavic genitive: only 1/21/31 take the singular, the rest the genitive plural.
pub(super) fn slavic_genitive(n: u64, one: &'static str, many: &'static str) -> &'static str {
    slavic(n, one, many, many)
}

/// Arabic: 0, the dual, 3–10, and the rest.
pub(super) fn arabic(
    n: u64,
    zero: &'static str,
    dual: &'static str,
    few: &'static str,
    many: &'static str,
) -> &'static str {
    let form = if n == 0 {
        0
    } else if n == 2 {
        1
    } else if (3..=10).contains(&n) {
        2
    } else if (11..=99).contains(&n) {
        3
    } else {
        match n % 100 {
            2 => 1,
            3..=10 => 2,
            _ => 3,
        }
    };
    [zero, dual, few, many][form]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(n: u64) -> &'static str {
        slavic(n, "one", "few", "many")
    }

    #[test]
    fn slavic_covers_ones() {
        assert_eq!(
            (0..=30).map(pick).collect::<Vec<_>>(),
            [
                "many", "one", "few", "few", "few", "many", "many", "many", "many", "many", "many",
                "many", "many", "many", "many", "many", "many", "many", "many", "many", "many",
                "one", "few", "few", "few", "many", "many", "many", "many", "many", "many",
            ]
        );
    }

    #[test]
    fn slavic_teens_use_many() {
        assert_eq!(pick(111), "many");
        assert_eq!(pick(112), "many");
        assert_eq!(pick(114), "many");
        assert_eq!(pick(121), "one");
        assert_eq!(pick(101), "one");
        assert_eq!(pick(100), "many");
    }

    #[test]
    fn slavic_genitive_only_ones() {
        assert_eq!(slavic_genitive(1, "one", "many"), "one");
        assert_eq!(slavic_genitive(21, "one", "many"), "one");
        assert_eq!(slavic_genitive(11, "one", "many"), "many");
        assert_eq!(slavic_genitive(3, "one", "many"), "many");
    }

    #[test]
    fn two_and_zero_or_one() {
        assert_eq!(two(1, "one", "many"), "one");
        assert_eq!(two(0, "one", "many"), "many");
        assert_eq!(zero_or_one(0, "one", "many"), "one");
        assert_eq!(zero_or_one(2, "one", "many"), "many");
    }

    #[test]
    fn arabic_forms() {
        let f = |n| arabic(n, "zero", "dual", "few", "many");
        assert_eq!(f(0), "zero");
        assert_eq!(f(1), "many");
        assert_eq!(f(2), "dual");
        assert_eq!(f(5), "few");
        assert_eq!(f(11), "many");
        assert_eq!(f(100), "many");
        assert_eq!(f(102), "dual");
        assert_eq!(f(205), "few");
        assert_eq!(f(111), "many");
    }
}
