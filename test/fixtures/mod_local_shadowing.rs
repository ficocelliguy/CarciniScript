// Inside an inline `mod`, a local named like one of the module's items shadows it only where it's
// in scope; elsewhere the name is the item (Rust by Example's `std/result/question_mark`).
mod checked {
    #[derive(Debug)]
    pub enum MathError {
        DivisionByZero,
        NonPositiveLogarithm,
    }

    type MathResult = Result<f64, MathError>;

    fn div(x: f64, y: f64) -> MathResult {
        if y == 0.0 { Err(MathError::DivisionByZero) } else { Ok(x / y) }
    }

    fn ln(x: f64) -> MathResult {
        if x <= 0.0 { Err(MathError::NonPositiveLogarithm) } else { Ok(x.ln()) }
    }

    fn double(x: i32) -> i32 {
        x * 2
    }

    fn op_(x: f64, y: f64) -> MathResult {
        // The binding isn't in scope in its own initializer.
        let ratio = div(x, y)?;
        let ln = ln(ratio)?;
        Ok(ln)
    }

    pub fn op(x: f64, y: f64) -> String {
        match op_(x, y) {
            // An enum path before `=>` inside macro arguments.
            Err(why) => format!("{}", match why {
                MathError::NonPositiveLogarithm => "logarithm of non-positive number",
                MathError::DivisionByZero => "division by zero",
            }),
            Ok(value) => format!("{:.3}", value),
        }
    }

    pub fn scopes(v: Option<i32>) -> Vec<i32> {
        let mut out = vec![];
        // Before a later `let double`, `double` is the function.
        out.push(double(1));
        if let Some(double) = v {
            out.push(double);
        } else {
            out.push(double(2));
        }
        for double in [10, 20] {
            out.push(double);
        }
        out.push(double(3));
        let f = |double: i32| double + 1;
        out.push(f(100));
        out.push(double(4));
        match v {
            Some(double) if double > 0 => out.push(double),
            _ => out.push(double(5)),
        }
        {
            let double = 7;
            out.push(double);
        }
        out.push(double(6));
        let double = 99;
        out.push(double);
        out
    }

    pub fn param(double: i32) -> i32 {
        double + 1
    }
}

fn main() {
    println!("{}", checked::op(10.0, 2.0));
    println!("{}", checked::op(1.0, 0.0));
    println!("{}", checked::op(-1.0, 2.0));
    println!("{:?}", checked::scopes(Some(3)));
    println!("{:?}", checked::scopes(None));
    println!("{}", checked::param(41));
}
