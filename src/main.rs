use std::{env, fs, io::stdout, process::ExitCode};

use linger::{interpreter::interp_program, parser::parse_program, tokenizer::tokenize, Writer};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: linger <FILE>");
        return ExitCode::FAILURE;
    }

    let linger_file_name = args[1].as_str();

    let linger_file_content = match fs::read_to_string(linger_file_name) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("error opening {linger_file_name}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let debug_tokens = false;
    let debug_program = false;
    let debug_value = false;

    let tokens = match tokenize(linger_file_content.as_str()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if debug_tokens {
        dbg!(&tokens);
        return ExitCode::FAILURE;
    }

    let program = match parse_program(tokens.as_slice()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if debug_program {
        dbg!(&program);
        return ExitCode::FAILURE;
    }

    let value = match interp_program(program, &mut Writer::new(Box::new(stdout()))) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if debug_value {
        dbg!(value);
        return ExitCode::SUCCESS;
    }

    return ExitCode::SUCCESS;
}
