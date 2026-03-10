use crate::display;
use crate::types::{Report, SessionReport};

fn csv_quote(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn print_csv_compact(report: &Report) {
    println!("date,input,output,cache_write,cache_read,thinking,total_tokens,cost");
    for s in &report.summaries {
        let total = s.total_input + s.total_output + s.total_cache() + s.total_thinking;
        println!(
            "{},{},{},{},{},{},{},{:.2}",
            csv_quote(&s.label),
            s.total_input,
            s.total_output,
            s.total_cache_creation(),
            s.total_cache_read(),
            s.total_thinking,
            total,
            s.total_cost
        );
    }
}

pub fn print_csv_breakdown(report: &Report) {
    println!("date,model,api_provider,client,input,output,cache_write,cache_read,thinking,total_tokens,cost");
    for s in &report.summaries {
        for m in &s.models {
            let model_total = m.total_tokens();
            println!(
                "{},{},{},{},{},{},{},{},{},{},{:.2}",
                csv_quote(&s.label),
                csv_quote(&display::display_model(&m.model)),
                csv_quote(display::infer_api_provider(m.effective_raw_model())),
                csv_quote(&display::display_client(&m.provider)),
                m.input_tokens,
                m.output_tokens,
                m.cache_creation_tokens,
                m.cache_read_tokens,
                m.thinking_tokens,
                model_total,
                m.cost_usd
            );
        }
    }
}

pub fn print_csv_sessions(report: &SessionReport) {
    println!("session_id,date,client,model,input,output,cache_write,cache_read,thinking,total_tokens,cost");
    for s in &report.sessions {
        let sid = if s.session_id.len() > 8 {
            &s.session_id[..8]
        } else {
            &s.session_id
        };
        println!(
            "{},{},{},{},{},{},{},{},{},{},{:.2}",
            csv_quote(sid),
            s.date.format("%Y-%m-%d"),
            csv_quote(&s.client),
            csv_quote(&s.dominant_model),
            s.input_tokens,
            s.output_tokens,
            s.cache_creation_tokens,
            s.cache_read_tokens,
            s.thinking_tokens,
            s.total_tokens,
            s.cost
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_quote_plain() {
        assert_eq!(csv_quote("hello"), "hello");
        assert_eq!(csv_quote("2026-02-20"), "2026-02-20");
    }

    #[test]
    fn test_csv_quote_with_comma() {
        assert_eq!(csv_quote("hello, world"), "\"hello, world\"");
    }

    #[test]
    fn test_csv_quote_with_quotes() {
        assert_eq!(csv_quote("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_csv_quote_with_newline() {
        assert_eq!(csv_quote("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn test_csv_quote_with_carriage_return() {
        assert_eq!(csv_quote("line1\r\nline2"), "\"line1\r\nline2\"");
        assert_eq!(csv_quote("text\r"), "\"text\r\"");
    }

    #[test]
    fn test_csv_quote_empty() {
        assert_eq!(csv_quote(""), "");
    }
}
