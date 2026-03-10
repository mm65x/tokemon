use crate::types::{Report, SessionReport};

pub fn print_json(report: &Report) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("[tokemon] Error serializing report: {e}"),
    }
}

pub fn print_sessions_json(report: &SessionReport) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("[tokemon] Error serializing sessions: {e}"),
    }
}
