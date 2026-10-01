//! `cargo xtask bench-startup [--runs N] [--record]`
//! Startup overhead of the Rust engine (rust-strict, so a missing addon fails) over the JS
//! engine: median of N interleaved `wait-on tcp:<local listener>` spawns per engine, gated
//! by benchmarks/startup-baseline.json's threshold. --record rewrites this host's entry.

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Number, Value, json};

use crate::host;

pub fn median(values: &[f64]) -> f64 {
    let mut s = values.to_vec();
    s.sort_by(f64::total_cmp);
    let mid = s.len() / 2;
    if s.len() % 2 == 1 {
        s[mid]
    } else {
        (s[mid - 1] + s[mid]) / 2.0
    }
}

pub struct Verdict {
    pub ok: bool,
    pub overhead_ms: f64,
    #[allow(dead_code)] // run() reports it through `message`; the tests read it directly
    pub allowed_ms: f64,
    pub message: String,
}

pub fn verdict(js_ms: f64, rust_ms: f64, relative: f64, floor_ms: f64) -> Verdict {
    let overhead_ms = rust_ms - js_ms;
    let allowed_ms = (relative * js_ms).max(floor_ms);
    let ok = overhead_ms <= allowed_ms;
    // ponytail: {:.1} rounds exact ties half-to-even where JS toFixed rounds up; ties are
    // exact binary halves (x.25, x.75), never seen in practice.
    let message = format!(
        "js median {js_ms:.1} ms, rust median {rust_ms:.1} ms, overhead {overhead_ms:.1} ms, allowed {allowed_ms:.1} ms: {}",
        if ok { "ok" } else { "FAIL" }
    );
    Verdict {
        ok,
        overhead_ms,
        allowed_ms,
        message,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub js_ms: Number,
    pub rust_ms: Number,
    pub overhead_ms: Number,
    pub date: String,
}

impl Sample {
    /// Values rounded to 0.1 as JS `Math.round(n * 10) / 10`.
    pub fn rounded(js_ms: f64, rust_ms: f64, overhead_ms: f64, date: &str) -> Self {
        Sample {
            js_ms: round1(js_ms),
            rust_ms: round1(rust_ms),
            overhead_ms: round1(overhead_ms),
            date: date.to_string(),
        }
    }
}

/// A whole result is an integer `Number`, so `100` is never written `100.0`.
fn round1(n: f64) -> Number {
    let r = (n * 10.0 + 0.5).floor() / 10.0; // JS Math.round: halves go up
    if r.fract() == 0.0 {
        Number::from(r as i64)
    } else {
        Number::from_f64(r).expect("finite timing")
    }
}

/// benchmarks/startup-baseline.json. Numbers stay `serde_json::Number` so an integer in
/// the file is written back as an integer.
#[derive(Clone, Debug, PartialEq)]
pub struct Baseline {
    pub relative: Number,
    pub floor_ms: Number,
    pub runs: Number,
    /// File order; a new host is appended, as JS `{...recorded, [key]: sample}` does.
    pub recorded: Vec<(String, Sample)>,
}

impl Baseline {
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let num = |v: &Value, at: &str| {
            v.as_number()
                .cloned()
                .ok_or_else(|| format!("baseline {at} must be a number"))
        };
        let recorded = v["recorded"]
            .as_object()
            .ok_or("baseline recorded must be an object")?
            .iter()
            .map(|(k, s)| {
                Ok((
                    k.clone(),
                    Sample {
                        js_ms: num(&s["jsMs"], "jsMs")?,
                        rust_ms: num(&s["rustMs"], "rustMs")?,
                        overhead_ms: num(&s["overheadMs"], "overheadMs")?,
                        date: s["date"]
                            .as_str()
                            .ok_or("baseline date must be a string")?
                            .to_string(),
                    },
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Baseline {
            relative: num(&v["threshold"]["relative"], "threshold.relative")?,
            floor_ms: num(&v["threshold"]["floorMs"], "threshold.floorMs")?,
            runs: num(&v["runs"], "runs")?,
            recorded,
        })
    }

    /// `JSON.stringify(baseline, null, 2) + '\n'`.
    pub fn to_json(&self) -> String {
        let recorded: Map<String, Value> = self
            .recorded
            .iter()
            .map(|(k, s)| {
                let sample = json!({
                    "jsMs": s.js_ms,
                    "rustMs": s.rust_ms,
                    "overheadMs": s.overhead_ms,
                    "date": s.date,
                });
                (k.clone(), sample)
            })
            .collect();
        let v = json!({
            "threshold": { "relative": self.relative, "floorMs": self.floor_ms },
            "runs": self.runs,
            "recorded": recorded,
        });
        serde_json::to_string_pretty(&v).expect("serialisable") + "\n"
    }
}

/// Replace `key`'s entry in place, or append it.
pub fn with_recording(baseline: &Baseline, key: &str, sample: Sample) -> Baseline {
    let mut next = baseline.clone();
    match next.recorded.iter_mut().find(|(k, _)| k == key) {
        Some((_, s)) => *s = sample,
        None => next.recorded.push((key.to_string(), sample)),
    }
    next
}

/// The subcommand: returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match bench(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn bench(args: &[String]) -> Result<i32, String> {
    let root = host::repo_root();
    let file = root.join("benchmarks").join("startup-baseline.json");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let baseline = Baseline::parse(&text)?;
    let (runs, record) = parse_args(args, baseline.runs.as_u64().unwrap_or(0))?;

    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    // ponytail: the accept thread lives until the process exits; nothing else needs the port.
    std::thread::spawn(move || listener.incoming().for_each(drop));

    let wait_on = root.join("bin").join("wait-on");
    let time = |engine: &str| -> Result<f64, String> {
        let start = Instant::now();
        let out = Command::new(host::node_exe())
            .arg(&wait_on)
            .args([format!("tcp:127.0.0.1:{port}"), "-t".into(), "10000".into()])
            .current_dir(&root)
            .env("WAIT_ON_ENGINE", engine)
            .stdin(Stdio::null())
            .output();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        match out {
            Ok(o) if o.status.success() => Ok(elapsed),
            Ok(o) => Err(format!(
                "WAIT_ON_ENGINE={engine} run failed (exit {}): {}",
                o.status.code().map_or("signal".into(), |c| c.to_string()),
                String::from_utf8_lossy(&o.stderr)
            )),
            Err(e) => Err(format!("WAIT_ON_ENGINE={engine} run failed (exit {e})")),
        }
    };

    const ENGINES: [&str; 2] = ["js", "rust-strict"];
    for engine in ENGINES {
        time(engine)?; // untimed warm-up
    }
    let (mut js, mut rust) = (Vec::new(), Vec::new());
    for _ in 0..runs {
        // interleaved so runner drift hits both engines equally
        js.push(time(ENGINES[0])?);
        rust.push(time(ENGINES[1])?);
    }

    let (js_ms, rust_ms) = (median(&js), median(&rust));
    let f = |n: &Number| n.as_f64().unwrap_or(f64::NAN);
    let v = verdict(js_ms, rust_ms, f(&baseline.relative), f(&baseline.floor_ms));
    println!("bench:startup ({runs} runs per engine) {}", v.message);
    if record {
        let (platform, arch) = host::node_name(std::env::consts::OS, std::env::consts::ARCH)?;
        let key = format!("{platform}-{arch}");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        let sample = Sample::rounded(js_ms, rust_ms, v.overhead_ms, &civil_date(now.as_secs()));
        std::fs::write(&file, with_recording(&baseline, &key, sample).to_json())
            .map_err(|e| format!("{}: {e}", file.display()))?;
        println!("recorded {key} in benchmarks/startup-baseline.json");
    }
    Ok(if v.ok { 0 } else { 1 })
}

/// `(runs, record)` from `[--runs N] [--record]`; runs defaults to the baseline's.
pub fn parse_args(args: &[String], default_runs: u64) -> Result<(u64, bool), String> {
    let (mut runs, mut record) = (default_runs, false);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--record" => record = true,
            "--runs" => {
                let v = it.next().map_or("", String::as_str);
                runs = v
                    .parse()
                    .ok()
                    .filter(|&n| n >= 1)
                    .ok_or_else(|| format!("--runs must be a positive integer, got {v}"))?;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok((runs, record))
}

/// UTC `YYYY-MM-DD` for Unix epoch seconds.
pub fn civil_date(epoch_secs: u64) -> String {
    // Howard Hinnant's civil_from_days, for days on or after 1970-01-01.
    let z = epoch_secs / 86_400 + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(js: f64, rust: f64, overhead: f64, date: &str) -> Sample {
        Sample::rounded(js, rust, overhead, date)
    }

    fn baseline(recorded: Vec<(String, Sample)>) -> Baseline {
        Baseline {
            relative: Number::from_f64(0.25).unwrap(),
            floor_ms: Number::from(50),
            runs: Number::from(20),
            recorded,
        }
    }

    #[test]
    fn with_recording_adds_the_host_and_keeps_others() {
        let linux = sample(1.0, 2.0, 1.0, "2026-09-01");
        let darwin = sample(3.0, 4.0, 1.0, "2026-09-30");
        let before = baseline(vec![("linux-x64".into(), linux.clone())]);
        let next = with_recording(&before, "darwin-arm64", darwin.clone());
        assert_eq!(
            next,
            baseline(vec![
                ("linux-x64".into(), linux.clone()),
                ("darwin-arm64".into(), darwin)
            ])
        );
        assert_eq!(before, baseline(vec![("linux-x64".into(), linux)]));
    }

    #[test]
    fn committed_baseline_round_trips_byte_identical() {
        let path = host::repo_root()
            .join("benchmarks")
            .join("startup-baseline.json");
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(Baseline::parse(&text).unwrap().to_json(), text);
    }

    #[test]
    fn recording_keeps_non_lexicographic_key_order_and_untouched_bytes() {
        let text = "{\n  \"threshold\": {\n    \"relative\": 0.25,\n    \"floorMs\": 50\n  },\n  \"runs\": 20,\n  \"recorded\": {\n    \"win32-x64\": {\n      \"jsMs\": 120.3,\n      \"rustMs\": 125,\n      \"overheadMs\": 4.7,\n      \"date\": \"2026-09-02\"\n    },\n    \"darwin-arm64\": {\n      \"jsMs\": 99.5,\n      \"rustMs\": 102.2,\n      \"overheadMs\": 2.7,\n      \"date\": \"2026-09-30\"\n    }\n  }\n}\n";
        let before = Baseline::parse(text).unwrap();
        let out = with_recording(
            &before,
            "linux-x64",
            sample(80.04, 90.0, 9.96, "2026-10-01"),
        )
        .to_json();
        let tail = "    \"linux-x64\": {\n      \"jsMs\": 80,\n      \"rustMs\": 90,\n      \"overheadMs\": 10,\n      \"date\": \"2026-10-01\"\n    }\n  }\n}\n";
        let untouched = &text[..text.len() - "\n  }\n}\n".len()];
        assert_eq!(out, format!("{untouched},\n{tail}"));
    }

    #[test]
    fn whole_rounded_values_serialise_as_integers() {
        let out = baseline(vec![("k".into(), sample(100.0, 102.7, 2.7, "d"))]).to_json();
        assert!(out.contains("\"jsMs\": 100,"), "{out}");
        assert!(out.contains("\"rustMs\": 102.7,"), "{out}");
        assert!(out.contains("\"overheadMs\": 2.7,"), "{out}");
    }

    #[test]
    fn median_of_odd_even_and_unsorted_lists() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&[7.0]), 7.0);
    }

    #[test]
    fn verdict_passes_under_the_relative_allowance() {
        let v = verdict(200.0, 240.0, 0.25, 50.0);
        assert!(v.ok);
        assert_eq!(v.allowed_ms, 50.0);
        assert_eq!(v.overhead_ms, 40.0);
    }

    #[test]
    fn verdict_passes_under_the_floor_when_relative_is_smaller() {
        let v = verdict(100.0, 145.0, 0.25, 50.0);
        assert!(v.ok);
        assert_eq!(v.allowed_ms, 50.0);
    }

    #[test]
    fn verdict_fails_past_both_naming_the_numbers() {
        let v = verdict(100.0, 160.0, 0.25, 50.0);
        assert!(!v.ok);
        for n in ["60", "50", "100", "160", "FAIL"] {
            assert!(v.message.contains(n), "{n} missing from {}", v.message);
        }
    }

    #[test]
    fn civil_date_from_fixed_epoch_seconds() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(1_790_726_400), "2026-09-30");
        assert_eq!(civil_date(1_709_164_800), "2024-02-29");
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn runs_must_be_a_positive_integer() {
        for v in ["0", "-1", "abc"] {
            assert_eq!(
                parse_args(&args(&["--runs", v]), 20),
                Err(format!("--runs must be a positive integer, got {v}"))
            );
        }
    }

    #[test]
    fn runs_defaults_to_the_baseline_and_record_is_a_flag() {
        assert_eq!(parse_args(&[], 20), Ok((20, false)));
        assert_eq!(
            parse_args(&args(&["--runs", "5", "--record"]), 20),
            Ok((5, true))
        );
    }
}
