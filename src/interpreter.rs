use std::{collections::HashMap, fmt};

use crate::{
    error::{
        LingerError::{self, RuntimeError},
        RuntimeError::*,
    },
    parser::{BinaryOperator, Expr, Procedure, Program, Statement, Statements},
};

#[derive(Copy, Clone, Debug)]
pub enum Value {
    Num(i64),
    Bool(bool),
    Void,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Num(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Void => write!(f, "<void>"),
        }
    }
}

pub type Environment = HashMap<String, Value>;

pub fn interp_program<'a>(p: Program<'a>) -> Result<Value, LingerError<'a>> {
    return interp_statements(&p.procedures, Environment::new(), p.main);
}

fn interp_statements<'a>(
    procs: &Vec<Procedure<'a>>,
    env: Environment,
    statements: Statements<'a>,
) -> Result<Value, LingerError<'a>> {
    let mut env = env;
    let mut return_value = Value::Void;
    for statement in statements {
        // TODO: add immediate termination if the statement is a return statement
        let (new_env, value) = match interp_statement(&procs, env.clone(), statement) {
            Ok(pair) => pair,
            Err(e) => return Err(e),
        };
        env = new_env;
        return_value = value;
    }
    return Ok(return_value);
}

pub fn interp_statement<'a>(
    procs: &Vec<Procedure<'a>>,
    env: Environment,
    statement: Statement<'a>,
) -> Result<(Environment, Value), LingerError<'a>> {
    match statement {
        Statement::Expr(expr) => match interp_expression(&procs, env.clone(), expr) {
            Ok(value) => Ok((env.clone(), value)),
            Err(e) => Err(e),
        },
        Statement::Let(id, let_expr) => match interp_expression(&procs, env.clone(), let_expr) {
            Ok(value) => {
                let mut env = env.clone();
                env.insert(id.to_string(), value);
                Ok((env, Value::Void))
            }
            Err(e) => Err(e),
        },
        Statement::If(cond_expr, then_statements, else_statements_option) => {
            let cond_value = match interp_expression(&procs, env.clone(), cond_expr) {
                Ok(value) => value,
                Err(e) => return Err(e),
            };
            match cond_value {
                Value::Bool(b) => {
                    if b {
                        match interp_statements(procs, env.clone(), then_statements) {
                            Ok(value) => return Ok((env.clone(), value)),
                            Err(e) => return Err(e),
                        };
                    } else {
                        match else_statements_option {
                            Some(else_statements) => {
                                match interp_statements(procs, env.clone(), else_statements) {
                                    Ok(value) => return Ok((env.clone(), value)),
                                    Err(e) => return Err(e),
                                };
                            }
                            None => Ok((env.clone(), Value::Void)),
                        }
                    }
                }
                v => return Err(RuntimeError(BadCondition(v))),
            }
        }
        Statement::Return(expr) => match interp_expression(&procs, env.clone(), expr) {
            Ok(value) => Ok((env, value)),
            Err(_) => todo!(),
        },
    }
}

pub fn interp_expression<'a>(
    procs: &Vec<Procedure<'a>>,
    env: Environment,
    expr: Expr<'a>,
) -> Result<Value, LingerError<'a>> {
    match expr {
        Expr::Num(n) => Ok(Value::Num(n)),
        Expr::Bool(b) => Ok(Value::Bool(b)),
        Expr::Var(id) => match env.get(id) {
            Some(value) => Ok(*value),
            None => Err(RuntimeError(UnknownVariable(id.to_string()))),
        },
        Expr::Binary(op, left, right) => {
            let left_value = match interp_expression(procs, env.clone(), *left) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            let right_value = match interp_expression(procs, env.clone(), *right) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            match op {
                BinaryOperator::Plus => match (left_value, right_value) {
                    (Value::Num(num_left), Value::Num(num_right)) => {
                        Ok(Value::Num(num_left + num_right))
                    }
                    // TODO: look into better methods of returning errors
                    (Value::Num(_), v) => Err(RuntimeError(BadArg(v))),
                    (v, _) => Err(RuntimeError(BadArg(v))),
                },
                BinaryOperator::Minus => match (left_value, right_value) {
                    (Value::Num(num_left), Value::Num(num_right)) => {
                        Ok(Value::Num(num_left - num_right))
                    }
                    // TODO: look into better methods of returning errors
                    (Value::Num(_), v) => Err(RuntimeError(BadArg(v))),
                    (v, _) => Err(RuntimeError(BadArg(v))),
                },
                BinaryOperator::Eq => match (left_value, right_value) {
                    (Value::Num(num_left), Value::Num(num_right)) => {
                        Ok(Value::Num(num_left + num_right))
                    }
                    (Value::Bool(bool_left), Value::Bool(bool_right)) => {
                        Ok(Value::Bool(bool_left == bool_right))
                    }
                    // TODO: determine if the arg types are mismatched, the first argument must be incorrect?
                    (v, _) => Err(RuntimeError(BadArg(v))),
                },
                BinaryOperator::LogicOr => match (left_value, right_value) {
                    (Value::Bool(bool_left), Value::Bool(bool_right)) => {
                        Ok(Value::Bool(bool_left || bool_right))
                    }
                    // TODO: look into better methods of returning errors
                    (Value::Bool(_), v) => Err(RuntimeError(BadArg(v))),
                    (v, _) => Err(RuntimeError(BadArg(v))),
                },
            }
        }
        Expr::Call(proc_name, args) => {
            let proc = match procs.iter().find(|p| p.name.eq(proc_name)) {
                Some(proc) => proc,
                None => return Err(RuntimeError(UnknownProc(proc_name.to_string()))),
            };

            if proc.params.len() != args.len() {
                return Err(RuntimeError(ArgMismatch(
                    proc_name.to_string(),
                    args.len(),
                    proc.params.len(),
                )));
            }

            let mut values: Vec<Value> = vec![];
            for expr in args {
                match interp_expression(procs, env.clone(), expr) {
                    Ok(v) => values.push(v),
                    Err(e) => return Err(e),
                }
            }

            let mut env = env.clone();
            let bindings = proc.params.iter().zip(values);
            for (param, value) in bindings {
                env.insert(param.to_string(), value);
            }

            return interp_statements(procs, env.clone(), proc.body.to_vec());
        }
    }
}
