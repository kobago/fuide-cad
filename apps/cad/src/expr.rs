//! Tiny arithmetic expressions for parameters: numbers, `+ - * /`, parentheses, unary minus,
//! named parameters and a few functions (`sqrt`, `abs`, `sin`, `cos`, `tan` in degrees,
//! `min`, `max`). `width / 2 + 3` is the whole point; nothing more.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ExprError(pub String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Evaluate `src`; `vars` resolves parameter names.
pub fn eval(src: &str, vars: &dyn Fn(&str) -> Option<f64>) -> Result<f64, ExprError> {
    let mut p = Parser {
        src: src.as_bytes(),
        pos: 0,
        vars,
    };
    p.skip_ws();
    if p.pos == p.src.len() {
        return Err(ExprError("empty expression".into()));
    }
    let v = p.expr()?;
    p.skip_ws();
    if p.pos != p.src.len() {
        return Err(ExprError(format!(
            "unexpected '{}' at {}",
            p.src[p.pos] as char,
            p.pos + 1
        )));
    }
    if !v.is_finite() {
        return Err(ExprError("result is not finite".into()));
    }
    Ok(v)
}

/// A literal number without parameters or arithmetic, or `None`.
pub fn literal(src: &str) -> Option<f64> {
    src.trim().parse().ok()
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    vars: &'a dyn Fn(&str) -> Option<f64>,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.src.get(self.pos).copied()
    }
    fn expr(&mut self) -> Result<f64, ExprError> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.pos += 1;
                    v += self.term()?;
                }
                Some(b'-') => {
                    self.pos += 1;
                    v -= self.term()?;
                }
                _ => return Ok(v),
            }
        }
    }
    fn term(&mut self) -> Result<f64, ExprError> {
        let mut v = self.unary()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    v *= self.unary()?;
                }
                Some(b'/') => {
                    self.pos += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err(ExprError("division by zero".into()));
                    }
                    v /= d;
                }
                _ => return Ok(v),
            }
        }
    }
    fn unary(&mut self) -> Result<f64, ExprError> {
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                Ok(-self.unary()?)
            }
            Some(b'+') => {
                self.pos += 1;
                self.unary()
            }
            _ => self.primary(),
        }
    }
    fn primary(&mut self) -> Result<f64, ExprError> {
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') {
                    return Err(ExprError(format!("expected ')' at {}", self.pos + 1)));
                }
                self.pos += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => self.ident(),
            Some(c) => Err(ExprError(format!(
                "unexpected '{}' at {}",
                c as char,
                self.pos + 1
            ))),
            None => Err(ExprError("unexpected end of expression".into())),
        }
    }
    fn number(&mut self) -> Result<f64, ExprError> {
        let start = self.pos;
        while self.pos < self.src.len()
            && (self.src[self.pos].is_ascii_digit() || self.src[self.pos] == b'.')
        {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("");
        text.parse()
            .map_err(|_| ExprError(format!("bad number '{text}'")))
    }
    fn ident(&mut self) -> Result<f64, ExprError> {
        let start = self.pos;
        while self.pos < self.src.len()
            && (self.src[self.pos].is_ascii_alphanumeric() || self.src[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let name = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("");
        if self.peek() == Some(b'(') {
            self.pos += 1;
            let mut args = vec![self.expr()?];
            while self.peek() == Some(b',') {
                self.pos += 1;
                args.push(self.expr()?);
            }
            if self.peek() != Some(b')') {
                return Err(ExprError(format!("expected ')' after {name}(")));
            }
            self.pos += 1;
            return function(name, &args);
        }
        (self.vars)(name).ok_or_else(|| ExprError(format!("unknown parameter '{name}'")))
    }
}

fn function(name: &str, args: &[f64]) -> Result<f64, ExprError> {
    let one = || {
        if args.len() == 1 {
            Ok(args[0])
        } else {
            Err(ExprError(format!("{name}() takes one argument")))
        }
    };
    match name {
        "sqrt" => {
            let x = one()?;
            if x < 0.0 {
                return Err(ExprError("sqrt of a negative number".into()));
            }
            Ok(x.sqrt())
        }
        "abs" => Ok(one()?.abs()),
        "sin" => Ok(one()?.to_radians().sin()),
        "cos" => Ok(one()?.to_radians().cos()),
        "tan" => Ok(one()?.to_radians().tan()),
        "min" | "max" if !args.is_empty() => {
            Ok(args.iter().copied().fold(
                args[0],
                |a, b| if name == "min" { a.min(b) } else { a.max(b) },
            ))
        }
        _ => Err(ExprError(format!("unknown function '{name}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> Option<f64> {
        match name {
            "width" => Some(40.0),
            "h" => Some(10.0),
            _ => None,
        }
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(eval("1 + 2 * 3", &v).unwrap(), 7.0);
        assert_eq!(eval("(1 + 2) * 3", &v).unwrap(), 9.0);
        assert_eq!(eval("-width / 2 + 3", &v).unwrap(), -17.0);
        assert_eq!(eval("2 * -h", &v).unwrap(), -20.0);
        assert_eq!(eval("10 - 2 - 3", &v).unwrap(), 5.0);
        assert_eq!(eval("  12.5  ", &v).unwrap(), 12.5);
    }

    #[test]
    fn functions() {
        assert_eq!(eval("sqrt(16) + abs(-2)", &v).unwrap(), 6.0);
        assert!((eval("sin(30)", &v).unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(eval("max(1, h, 3)", &v).unwrap(), 10.0);
        assert_eq!(eval("min(width, h)", &v).unwrap(), 10.0);
    }

    #[test]
    fn errors_name_the_problem() {
        assert_eq!(eval("", &v).unwrap_err().0, "empty expression");
        assert_eq!(
            eval("depth * 2", &v).unwrap_err().0,
            "unknown parameter 'depth'"
        );
        assert_eq!(eval("1 / 0", &v).unwrap_err().0, "division by zero");
        assert_eq!(eval("(1 + 2", &v).unwrap_err().0, "expected ')' at 7");
        assert_eq!(eval("1 2", &v).unwrap_err().0, "unexpected '2' at 3");
        assert_eq!(eval("foo(1)", &v).unwrap_err().0, "unknown function 'foo'");
        assert_eq!(
            eval("sqrt(-1)", &v).unwrap_err().0,
            "sqrt of a negative number"
        );
    }

    #[test]
    fn literal_detection() {
        assert_eq!(literal(" 5 "), Some(5.0));
        assert_eq!(literal("width"), None);
        assert_eq!(literal("1+1"), None);
    }
}
