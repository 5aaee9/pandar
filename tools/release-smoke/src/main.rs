use std::{env, process::ExitCode};

fn main() -> ExitCode {
    match pandar_release_smoke::run(env::args().skip(1)) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("release smoke failed: {error}");
            ExitCode::FAILURE
        }
    }
}
