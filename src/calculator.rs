//! Expression evaluation using fend.
//!
//! Wraps fend to provide a simple interface for evaluating
//! mathematical expressions and formatting results.

#[derive(Clone, Debug, PartialEq)]
pub struct CalculatorItem {
    pub expression: String,
    pub display_result: String,
    pub clipboard_result: Option<String>,
    pub is_error: bool,
}

impl CalculatorItem {
    pub fn text_for_clipboard(&self) -> &str {
        self.clipboard_result
            .as_deref()
            .unwrap_or(&self.display_result)
    }
}
use fend_core::Context;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Interrupt handler that stops fend evaluation after a timeout.
struct TimeoutInterrupt {
    deadline: Instant,
}

impl TimeoutInterrupt {
    fn new(timeout: Duration) -> Self {
        Self {
            deadline: Instant::now() + timeout,
        }
    }
}

impl fend_core::Interrupt for TimeoutInterrupt {
    fn should_interrupt(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

static CONTEXT: OnceLock<Mutex<Context>> = OnceLock::new();

/// Evaluate a mathematical expression.
///
/// Returns `Ok(CalculatorItem)` if the expression can be parsed,
/// or an error if parsing fails.
pub fn evaluate_expression(input: &str) -> Result<CalculatorItem, String> {
    let expression = input.trim().to_string();

    let mut context = CONTEXT
        .get_or_init(|| Mutex::new(Context::new()))
        .lock()
        .unwrap();
    let interrupt = TimeoutInterrupt::new(Duration::from_millis(50));
    match fend_core::evaluate_with_interrupt(&expression, &mut context, &interrupt) {
        Ok(value) => {
            let value = value.get_main_result();
            let calc_value = value.trim_start_matches("approx. ");
            Ok(CalculatorItem {
                expression,
                display_result: format_display(value),
                clipboard_result: Some(calc_value.to_string()),
                is_error: false,
            })
        }
        Err(err) => {
            if err == "division by zero" {
                Ok(CalculatorItem {
                    expression,
                    display_result: "Infinity".to_string(),
                    clipboard_result: None,
                    is_error: true,
                })
            } else {
                Err(err.to_string())
            }
        }
    }
}

/// Format a number for display with thousand separators.
fn format_display(value: &str) -> String {
    if let Some((integer, fraction)) = value.split_once('.') {
        if !fraction.is_empty() && fraction.bytes().all(|b| b.is_ascii_digit()) {
            if let Some(integer) = separate_exact_integer(integer) {
                return format!("{integer}.{fraction}");
            }
        }
        return value.to_string();
    }
    separate_exact_integer(value).unwrap_or_else(|| value.to_string())
}

/// Thousand separators for fend's exact integer strings.
fn separate_exact_integer(raw: &str) -> Option<String> {
    let (sign, digits) = match raw.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", raw),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut result = String::new();
    for (i, c) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    Some(format!(
        "{}{}",
        sign,
        result.chars().rev().collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::evaluate_expression;

    #[test]
    fn test_basic_evaluation() {
        let result = evaluate_expression("2 + 2").unwrap();
        assert_eq!(result.display_result, "4");
        assert_eq!(result.text_for_clipboard(), "4");
    }

    #[test]
    fn test_thousand_separators() {
        let result = evaluate_expression("1000 * 1000").unwrap();
        assert_eq!(result.display_result, "1,000,000");
        assert_eq!(result.text_for_clipboard(), "1000000");
    }

    #[test]
    fn test_decimal_result() {
        let result = evaluate_expression("1 / 3").unwrap();
        // Should have decimal places, no trailing zeros
        assert!(result.display_result.starts_with("approx. 0.333"));
    }

    #[test]
    fn test_division_by_zero() {
        let result = evaluate_expression("1 / 0").unwrap();
        assert_eq!(result.display_result, "Infinity");
    }

    #[test]
    fn test_invalid_expression() {
        // Invalid fend syntax.
        let result = evaluate_expression("2 +* 2");
        assert!(result.is_err());
    }

    #[test]
    fn test_functions() {
        // Exponentiation also supports fractional powers.
        let result = evaluate_expression("16^0.5").unwrap();
        assert_eq!(result.display_result, "4");
    }

    #[test]
    fn test_trig_functions() {
        let result = evaluate_expression("sin(0)").unwrap();
        assert_eq!(result.display_result, "0");
    }
}

#[cfg(test)]
mod sign_tests {
    use super::evaluate_expression;

    #[test]
    fn negative_proper_fraction_keeps_its_sign() {
        // fend returns the exact string "-0.5"; display must not drop the minus.
        let result = evaluate_expression("0 - 0.5").unwrap();
        assert_eq!(result.display_result, "-0.5");
        assert_eq!(result.text_for_clipboard(), "-0.5");
        let result = evaluate_expression("-3 / 4").unwrap();
        assert_eq!(result.display_result, "-0.75");
    }
}

#[cfg(test)]
mod big_small_tests {
    use super::evaluate_expression;

    #[test]
    fn huge_integers_do_not_collapse_to_zero() {
        let result = evaluate_expression("2^100").unwrap();
        assert_eq!(
            result.display_result,
            "1,267,650,600,228,229,401,496,703,205,376"
        );
        assert_eq!(
            result.text_for_clipboard(),
            "1267650600228229401496703205376"
        );
        let result = evaluate_expression("-2^100").unwrap();
        assert_eq!(
            result.display_result,
            "-1,267,650,600,228,229,401,496,703,205,376"
        );
    }

    #[test]
    fn tiny_fractions_do_not_round_to_zero() {
        let result = evaluate_expression("10^-11").unwrap();
        assert_eq!(result.display_result, "0.00000000001");
        assert_eq!(result.text_for_clipboard(), "0.00000000001");
        let result = evaluate_expression("-10^-11").unwrap();
        assert_eq!(result.display_result, "-0.00000000001");
    }
}

#[cfg(test)]
mod exact_decimal_tests {
    use super::{evaluate_expression, format_display};

    #[test]
    fn decimal_display_keeps_fends_exact_digits() {
        for (input, display) in [
            ("99999999999999.99", "99,999,999,999,999.99"),
            ("123456789.123456789", "123,456,789.123456789"),
            ("-99999999999999.99", "-99,999,999,999,999.99"),
            ("-0.123456789012345", "-0.123456789012345"),
            ("999999999999999.99", "999,999,999,999,999.99"),
        ] {
            let item = evaluate_expression(input).unwrap();
            assert_eq!(item.display_result, display);
            assert_eq!(item.text_for_clipboard(), input);
            assert!(!item.is_error);
        }
    }

    #[test]
    fn decimal_formatting_leaves_nonnumbers_unchanged() {
        for text in [
            "approx. 0.3333333333",
            "5000 m",
            "0 °C",
            "0 + i",
            "1e100",
            "NaN",
            "inf",
            "",
            "1.",
            ".5",
        ] {
            assert_eq!(format_display(text), text);
        }
        assert_eq!(format_display("0"), "0");
        assert_eq!(
            format_display("-0.0000000000000000001"),
            "-0.0000000000000000001"
        );
    }
}
