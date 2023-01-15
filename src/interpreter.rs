use std::fmt;

use crate::{
    desugar::{Expr, Statement},
    environment::{AssignmentType, Binding, Entry, Environment, Mutability},
    error::RuntimeError::{self, *},
    parser::Program,
    tokenizer::Operator,
};

#[derive(Clone, Debug)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
    Proc(Vec<String>, Statement, Environment),
    List(Vec<Value>),
    // ! consider if Nil should be an explicit value or just return an Option<Value> instead where None represents Nil
    Nil,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ControlFlow {
    Return,
    Normal,
    Break,
    Continue,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Num(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Nil => write!(f, "nil"),
            Value::Str(s) => write!(f, "{}", s),
            Value::Proc(..) => write!(f, "<lambda>"),
            Value::List(list) => {
                let values_as_strings: Vec<String> = list.iter().map(|v| v.to_string()).collect();
                let list_string = values_as_strings.join(", ");
                write!(f, "[{list_string}]")
            }
        }
    }
}

pub fn interp_program<'a>(p: Program) -> Result<Value, RuntimeError> {
    return match interp_statement(&mut Environment::new(p.procedures), p.main, false)? {
        (value, _) => Ok(value),
    };
}

pub fn interp_statement(
    env: &mut Environment,
    statement: Statement,
    in_loop: bool,
) -> Result<(Value, ControlFlow), RuntimeError> {
    match statement {
        Statement::Expr(expr) => match interp_expression(env, expr)? {
            value => Ok((value, ControlFlow::Normal)),
        },
        Statement::Let(id, new_expr) => {
            let new_value = interp_expression(env, new_expr)?;
            env.insert_new_mutable_value(id, new_value);
            Ok((Value::Nil, ControlFlow::Normal))
        }
        Statement::Const(id, new_expr) => {
            let new_value = interp_expression(env, new_expr)?;
            env.insert_new_constant_value(id, new_value);
            Ok((Value::Nil, ControlFlow::Normal))
        }
        Statement::Assign(id, expr) => {
            let value = interp_expression(env, expr)?;
            env.reassign(id, value)?;
            Ok((Value::Nil, ControlFlow::Normal))
        }
        Statement::If(cond_expr, then_statement, else_statement_option) => {
            let cond_bool = match interp_expression(env, cond_expr)? {
                Value::Bool(b) => b,
                v => return Err(BadArg(v)),
            };
            if cond_bool {
                interp_statement(env, *then_statement, in_loop)
            } else {
                match else_statement_option {
                    Some(else_statement) => interp_statement(env, *else_statement, in_loop),
                    None => Ok((Value::Nil, ControlFlow::Normal)),
                }
            }
        }
        Statement::While(cond_expr, while_block) => Ok(loop {
            let cond_bool = match interp_expression(env, cond_expr.clone())? {
                Value::Bool(b) => b,
                v => return Err(BadArg(v)),
            };
            if cond_bool {
                match interp_statement(env, *while_block.clone(), true)? {
                    (value, ControlFlow::Return) => break (value, ControlFlow::Return),
                    (_, ControlFlow::Break) => break (Value::Nil, ControlFlow::Normal),
                    (_, ControlFlow::Normal) => (),
                    (_, ControlFlow::Continue) => (),
                };
            } else {
                break (Value::Nil, ControlFlow::Normal);
            }
        }),
        Statement::Return(expr_option) => match expr_option {
            Some(expr) => Ok((interp_expression(env, expr)?, ControlFlow::Return)),
            None => Ok((Value::Nil, ControlFlow::Return)),
        },
        Statement::Break => Ok((Value::Nil, ControlFlow::Break)),
        Statement::Continue => Ok((Value::Nil, ControlFlow::Continue)),
        Statement::Block(statements) => {
            let mut block_value = Value::Nil;
            let mut block_env = env.clone();
            for statement in statements {
                let statement_value = match interp_statement(&mut block_env, statement, in_loop)? {
                    (value, ControlFlow::Normal) => value,
                    (value, ControlFlow::Return) => {
                        env.update_reassigned_entries(&block_env)?;
                        return Ok((value, ControlFlow::Return));
                    }
                    (value, ControlFlow::Break) => {
                        if in_loop {
                            env.update_reassigned_entries(&block_env)?;
                            return Ok((value, ControlFlow::Break));
                        } else {
                            return Err(BreakNotInLoop);
                        }
                    }
                    (value, ControlFlow::Continue) => {
                        if in_loop {
                            env.update_reassigned_entries(&block_env)?;
                            return Ok((value, ControlFlow::Continue));
                        } else {
                            return Err(ContinueNotInLoop);
                        }
                    }
                };
                block_value = statement_value;
            }
            env.update_reassigned_entries(&block_env)?;
            return Ok((block_value, ControlFlow::Normal));
        }
    }
}

fn interp_expression<'a>(env: &mut Environment, expr: Expr) -> Result<Value, RuntimeError> {
    match expr {
        Expr::Nil => Ok(Value::Nil),
        Expr::Num(n) => Ok(Value::Num(n)),
        Expr::Bool(b) => Ok(Value::Bool(b)),
        Expr::Str(s) => Ok(Value::Str(s)),
        Expr::Lambda(params, body) => Ok(Value::Proc(params, *body, env.clone())),
        Expr::Var(id) => match env.get(id.to_string())? {
            v => Ok(v),
        },
        Expr::Binary(op, left, right) => match op {
            Operator::Plus => {
                match (
                    interp_expression(env, *left)?,
                    interp_expression(env, *right)?,
                ) {
                    (Value::Num(num_left), Value::Num(num_right)) => {
                        Ok(Value::Num(num_left + num_right))
                    }
                    (Value::Str(num_left), Value::Str(num_right)) => {
                        Ok(Value::Str(num_left + num_right.as_str()))
                    }
                    (Value::List(mut list_left), Value::List(mut list_right)) => {
                        list_left.append(&mut list_right);
                        Ok(Value::List(list_left))
                    }
                    (Value::Num(_), v) => Err(BadArg(v)),
                    (v, _) => Err(BadArg(v)),
                }
            }
            Operator::Minus => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Num(num_left - num_right))
                }
                (Value::Num(_), v) => Err(BadArg(v)),
                (v, _) => Err(BadArg(v)),
            },
            Operator::Eq => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Bool(num_left == num_right))
                }
                (Value::Bool(bool_left), Value::Bool(bool_right)) => {
                    Ok(Value::Bool(bool_left == bool_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::Ne => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Bool(num_left != num_right))
                }
                (Value::Bool(bool_left), Value::Bool(bool_right)) => {
                    Ok(Value::Bool(bool_left != bool_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::LT => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Bool(num_left < num_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::GT => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Bool(num_left > num_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::LTE => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Bool(num_left <= num_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::GTE => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Bool(num_left >= num_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::LogicOr => match interp_expression(env, *left)? {
                Value::Bool(b) => match b {
                    true => Ok(Value::Bool(true)),
                    false => match interp_expression(env, *right)? {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        right_value => Err(BadArg(right_value)),
                    },
                },
                left_value => Err(BadArg(left_value)),
            },
            Operator::LogicAnd => match interp_expression(env, *left)? {
                Value::Bool(b) => match b {
                    false => Ok(Value::Bool(false)),
                    true => match interp_expression(env, *right)? {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        right_value => Err(BadArg(right_value)),
                    },
                },
                left_value => Err(BadArg(left_value)),
            },
            Operator::Times => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Num(num_left * num_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::Mod => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Num(num_left % num_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            Operator::Div => match (
                interp_expression(env, *left)?,
                interp_expression(env, *right)?,
            ) {
                (Value::Num(num_left), Value::Num(num_right)) => {
                    Ok(Value::Num(num_left / num_right))
                }
                (v_left, v_right) => Err(BadArgs(vec![v_left, v_right])),
            },
            op => Err(UnaryAsBinary(op)),
        },
        Expr::Unary(op, operand) => match op {
            Operator::PreIncrement => {
                let var_name = match *operand {
                    Expr::Var(ref id) => id.to_string(),
                    _ => return Err(InvalidAssignmentTarget),
                };

                let num_value = match interp_expression(env, *operand)? {
                    Value::Num(n) => n,
                    v => return Err(BadArg(v)),
                };

                env.reassign(var_name, Value::Num(num_value + 1.0))?;

                return Ok(Value::Num(num_value + 1.0));
            }
            Operator::PostIncrement => {
                let var_name = match *operand {
                    Expr::Var(ref id) => id.to_string(),
                    _ => return Err(InvalidAssignmentTarget),
                };

                let original_num_value = match interp_expression(env, *operand)? {
                    Value::Num(n) => n,
                    v => return Err(BadArg(v)),
                };

                env.reassign(var_name, Value::Num(original_num_value + 1.0))?;

                return Ok(Value::Num(original_num_value));
            }
            Operator::PreDecrement => {
                let var_name = match *operand {
                    Expr::Var(ref id) => id.to_string(),
                    _ => return Err(InvalidAssignmentTarget),
                };

                let num_value = match interp_expression(env, *operand)? {
                    Value::Num(n) => n,
                    v => return Err(BadArg(v)),
                };

                env.reassign(var_name, Value::Num(num_value - 1.0))?;

                return Ok(Value::Num(num_value - 1.0));
            }
            Operator::PostDecrement => {
                let var_name = match *operand {
                    Expr::Var(ref id) => id.to_string(),
                    _ => return Err(InvalidAssignmentTarget),
                };

                let original_num_value = match interp_expression(env, *operand)? {
                    Value::Num(n) => n,
                    v => return Err(BadArg(v)),
                };

                env.reassign(var_name, Value::Num(original_num_value - 1.0))?;

                return Ok(Value::Num(original_num_value));
            }
            Operator::Minus => match interp_expression(env, *operand)? {
                Value::Num(n) => Ok(Value::Num(-n)),
                v => Err(BadArg(v)),
            },
            Operator::LogicNot => match interp_expression(env, *operand)? {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                v => Err(BadArg(v)),
            },
            op => Err(BinaryAsUnary(op)),
        },
        Expr::Call(f_expr, args) => {
            let f_name = match *f_expr {
                Expr::Var(ref f_name) => f_name.to_string(),
                _ => "<lambda>".to_string(),
            };

            let (f_params, f_body, f_env) = match interp_expression(env, *f_expr)? {
                Value::Proc(params, body, env) => (params, body, env),
                v => return Err(BadArg(v)),
            };

            if args.len() != f_params.len() {
                return Err(ArgMismatch(
                    f_name.to_string(),
                    f_params.len(), // expected
                    args.len(),     // actual
                ));
            }

            let arg_values_result: Result<Vec<Value>, RuntimeError> = args
                .into_iter()
                .map(|arg| interp_expression(env, arg))
                .collect();
            let arg_values = match arg_values_result {
                Ok(values) => values,
                Err(e) => return Err(e),
            };

            let entries: Vec<Entry> = arg_values
                .into_iter()
                .map(|v| (v, AssignmentType::Initialized, Mutability::Constant))
                .collect();

            let param_bindings: Vec<Binding> = f_params
                .iter()
                .map(|param| param.to_string())
                .zip(entries)
                .collect();

            return match interp_statement(&mut f_env.extend(param_bindings), f_body, false)? {
                (value, _) => Ok(value),
            };
        }
        Expr::PrimitiveCall(builtin, args) => match builtin {
            crate::parser::Builtin::Print => {
                let mut values: Vec<Value> = vec![];
                for expr in args {
                    values.push(interp_expression(env, expr)?);
                }
                let values: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                let values = values.join(" ");
                print!("{}", values);
                Ok(Value::Nil)
            }
            crate::parser::Builtin::List => {
                let mut values = vec![];
                for expr in args {
                    values.push(interp_expression(env, expr)?);
                }
                Ok(Value::List(values))
            }
            crate::parser::Builtin::IsEmpty => {
                if args.len() > 1 {
                    return Err(ArgMismatch("is_empty".to_string(), args.len(), 1));
                }

                let arg = match args.first() {
                    Some(arg) => arg.clone(),
                    None => return Err(ArgMismatch("is_empty".to_string(), 0, 1)),
                };

                match interp_expression(env, arg)? {
                    Value::List(list) => Ok(Value::Bool(list.is_empty())),
                    bad_arg => return Err(ExpectedList(bad_arg.to_string())),
                }
            }
            crate::parser::Builtin::IsNil => {
                if args.len() > 1 {
                    return Err(ArgMismatch("is_nil".to_string(), args.len(), 1));
                }

                let arg = match args.first() {
                    Some(arg) => arg.clone(),
                    None => return Err(ArgMismatch("is_nil".to_string(), 0, 1)),
                };

                match interp_expression(env, arg)? {
                    Value::Nil => Ok(Value::Bool(true)),
                    _ => Ok(Value::Bool(false)),
                }
            }
        },
        Expr::Index(indexable_expr, index_expr) => match interp_expression(env, *indexable_expr)? {
            Value::List(list) => {
                match interp_expression(env, *index_expr)? {
                    Value::Num(num) => {
                        if num.fract() != 0.0 {
                            return Err(ExpectedInteger(num.to_string()));
                        }

                        let index = num as i64;
                        if index < 0 || index as usize > list.len() - 1 {
                            return Err(IndexOutOfBounds(index));
                        }

                        return Ok(list[index as usize].clone());
                    }
                    bad_value => return Err(ExpectedInteger(bad_value.to_string())),
                };
            }
            Value::Str(_) => todo!(),
            value => return Err(NotIndexable(value.to_string())),
        },
    }
}
