use rust_decimal::Decimal;
use std::str::FromStr;

#[test]
fn parse_rejects_non_ascii() {
    let err = ferrum_ucum::parse("µg").unwrap_err();
    assert!(matches!(err, ferrum_ucum::Error::NonAscii));
}

#[test]
fn validate_rejects_invalid_syntax() {
    assert!(ferrum_ucum::validate("mg//dL").is_err());
    assert!(ferrum_ucum::validate("kg/(m.s2").is_err());
    assert!(ferrum_ucum::validate("m..s").is_err());
}

#[test]
fn equivalence_basic() {
    assert!(ferrum_ucum::equivalent("mg/dL", "g/L").unwrap());
    assert!(!ferrum_ucum::equivalent("mg", "m").unwrap());
}

#[test]
fn case_sensitive_symbols() {
    assert!(ferrum_ucum::validate("[iU]").is_ok());
    assert!(ferrum_ucum::validate("[IU]").is_ok());
    assert!(ferrum_ucum::validate("iu").is_err());
}

#[test]
fn converts_minutes_to_seconds() {
    let v = ferrum_ucum::convert_decimal(Decimal::ONE, "min", "s").unwrap();
    assert_eq!(v, Decimal::from(60));
}

#[test]
fn deciliter_to_liter() {
    let v = ferrum_ucum::convert_decimal(Decimal::ONE, "dL", "L").unwrap();
    assert_eq!(v, Decimal::from_str("0.1").unwrap());
}

#[test]
fn normalizes_pressure_to_pa() {
    let n = ferrum_ucum::normalize(Decimal::from(120), "mm[Hg]").unwrap();
    assert_eq!(n.unit, "Pa");
    assert_eq!(n.value, Decimal::from_str("15998.64").unwrap());
}
