use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);
}

#[wasm_bindgen]
pub fn greet(name: &str) {
    alert(&format!("Hello, {} from rust!", name));
}

#[wasm_bindgen]
pub fn compute_points(start: f64, end: f64, samples: usize, expr: &str) -> Vec<f32> {
    // defensive: clamp samples
    let samples = samples.max(2);
    // parse expression once
    let tokens = match tokenize(expr) {
        Ok(t) => t,
        Err(_) => return vec![], // on parse error return empty array; caller can handle
    };
    let rpn = match shunting_yard(&tokens) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut out: Vec<f32> = Vec::with_capacity(samples * 2);
    let dx = (end - start) / ((samples - 1) as f64);

    for i in 0..samples {
        let x = start + (i as f64) * dx;
        match eval_rpn(&rpn, x) {
            Ok(y) => {
                out.push(x as f32);
                out.push(y as f32);
            }
            Err(_) => {
                // pushing NaN for invalid eval so downstream can skip or break as desired
                out.push(x as f32);
                out.push(f32::NAN);
            }
        }
    }

    out
}

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(s: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => { chars.next(); }
            '0'..='9' | '.' => {
                let mut num = String::new();

                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' || d == 'e' || d == 'E' || d == '+' || d == '-' && num.ends_with('e') {
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }

                let v = num.parse::<f64>().map_err(|e| format!("num parse err: {}", e))?;
                out.push(Token::Number(v));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut id = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_alphanumeric() || d == '_' {
                        id.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
    
                out.push(Token::Ident(id));
            }
            '+' | '-' | '*' | '/' | '^' => {
                out.push(Token::Op(c));
                chars.next();
            }
            '(' => { out.push(Token::LParen); chars.next(); }
            ')' => { out.push(Token::RParen); chars.next(); }
            ',' => { out.push(Token::Comma); chars.next(); }
            _ => return Err(format!("unexpected char: {}", c)),
        }
    }

    Ok(out)
}

fn precedence(op: char) -> u8 {
    match op {
        '+' | '-' => 1,
        '*' | '/' => 2,
        '^' => 3,
        _ => 0,
    }
}

fn is_right_associative(op: char) -> bool {
    match op {
        '^' => true,
        _ => false,
    }
}

fn shunting_yard(tokens: &[Token]) -> Result<Vec<Token>, String> {
    let mut output: Vec<Token> = Vec::new();
    let mut ops: Vec<Token> = Vec::new();

    for tok in tokens.iter() {
        match tok {
            Token::Number(_) => output.push(tok.clone()),
            Token::Ident(name) => {
                // could be function or variable; push as identifier (function handling later)
                ops.push(Token::Ident(name.clone()));
            }
            Token::Comma => {
                // pop until left parenthesis is found
                while let Some(top) = ops.last() {
                    if matches!(top, Token::LParen) {
                        break;
                    } else {
                        output.push(ops.pop().unwrap());
                    }
                }
            }
            Token::Op(op1) => {
                while let Some(top) = ops.last() {
                    match top {
                        Token::Op(op2) => {
                            let p1 = precedence(*op1);
                            let p2 = precedence(*op2);
                            if (is_right_associative(*op1) && p1 < p2) || (!is_right_associative(*op1) && p1 <= p2) {
                                output.push(ops.pop().unwrap());
                            } else {
                                break;
                            }
                        }
                        Token::Ident(_) => {
                            // function on stack should be popped to output before operator
                            output.push(ops.pop().unwrap());
                        }
                        _ => break,
                    }
                }
                ops.push(tok.clone());
            }
            Token::LParen => ops.push(Token::LParen),
            Token::RParen => {
                // pop until LParen
                let mut found = false;
                while let Some(top) = ops.pop() {
                    if matches!(top, Token::LParen) {
                        found = true;
                        break;
                    } else {
                        output.push(top);
                    }
                }
                if !found {
                    return Err(')' .to_string());
                }
                // if top of ops is a function ident, pop it onto output
                if let Some(Token::Ident(_)) = ops.last() {
                    output.push(ops.pop().unwrap());
                }
            }
        }
    }

    while let Some(op) = ops.pop() {
        if matches!(op, Token::LParen) || matches!(op, Token::RParen) {
            return Err("mismatched parens".to_string());
        }
        output.push(op);
    }

    Ok(output)
}

fn eval_rpn(rpn: &[Token], x_val: f64) -> Result<f64, String> {
    let mut stack: Vec<f64> = Vec::new();

    for tok in rpn.iter() {
        match tok {
            Token::Number(v) => stack.push(*v),
            Token::Ident(name) => {
                // variable or function without parentheses (unary variable 'x' likely)
                if name == "x" {
                    stack.push(x_val);
                } else {
                    // treat as function with 1 argument by default
                    let arg = stack.pop().ok_or('f'.to_string())?;
                    let res = apply_func(name, arg)?;
                    stack.push(res);
                }
            }
            Token::Op(op) => {
                if *op == '-' && stack.len() == 1 {
                    // unary minus support (if parser created unary minus as Op and only one arg available)
                    let a = stack.pop().unwrap();
                    stack.push(-a);
                    continue;
                }
                let b = stack.pop().ok_or("stack underflow")?;
                let a = stack.pop().ok_or("stack underflow")?;
                let r = match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => a / b,
                    '^' => a.powf(b),
                    _ => return Err(format!("unknown op {}", op)),
                };
                stack.push(r);
            }
            Token::Comma => return Err("unexpected comma in rpn".to_string()),
            Token::LParen | Token::RParen => return Err("paren in rpn".to_string()),
        }
    }

    if stack.len() == 1 {
        Ok(stack[0])
    } else {
        Err("invalid expression evaluation".to_string())
    }
}

fn apply_func(name: &str, a: f64) -> Result<f64, String> {
    match name.to_lowercase().as_str() {
        "sin" => Ok(a.sin()),
        "cos" => Ok(a.cos()),
        "tan" => Ok(a.tan()),
        "asin" => Ok(a.asin()),
        "acos" => Ok(a.acos()),
        "atan" => Ok(a.atan()),
        "exp" => Ok(a.exp()),
        "ln" | "log" => Ok(a.ln()),
        "sqrt" => Ok(a.sqrt()),
        "abs" => Ok(a.abs()),
        _ => Err(format!("unknown func '{}'", name)),
    }
}