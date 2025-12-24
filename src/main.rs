use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::io::{BufRead, BufReader};

use tracing::span;
use tracing::Level;
use tracing_subscriber;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FmtSpan;

fn parse_temperature_line(line: &str) -> (String, f32) {
    let parts = line.split(";").collect::<Vec<&str>>();
    let city = parts.get(0).expect("should have the city part").to_string();
    let temperature = parts
        .get(1)
        .expect("should have temperature part")
        .parse::<f32>()
        .expect("temperature should be f32 parseable");

    (city, temperature)
}

#[tracing::instrument(skip_all)]
fn print_results(station_stats: HashMap<String, StationStats>, mut out_fd: &mut dyn Write) {
    let mut results = station_stats
        .into_iter()
        .map(|(key, value)| return (key, value))
        .collect::<Vec<(String, StationStats)>>();
    results.sort_by(|a, b| (a.0).cmp(&b.0));

    write!(&mut out_fd, "{{").expect("write to output file should suceed");
    for result in results {
        write!(
            &mut out_fd,
            "{}={}/{}/{}, ",
            result.0,
            result.1.min,
            result.1.sum / (result.1.count as f32),
            result.1.max,
        )
        .expect("write to output file should suceed");
    }
    write!(&mut out_fd, "}}").expect("write to output file should suceed");
}

pub struct StationStats {
    pub min: f32,
    pub sum: f32,
    pub max: f32,
    pub count: u64,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let data_file = args
        .get(1)
        .expect("data file should be passed as an argument");
    let out_file = args.get(2);

    println!("Running 1BRC on file {}", data_file);
    fmt::fmt()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(false)
        .with_level(false)
        .init();
    let program_span = span!(Level::INFO, "program");
    let program_span_guard = program_span.enter();

    let mut station_stats: HashMap<String, StationStats> = HashMap::new();

    let file = File::open(data_file).expect("should be able to open file for reading");
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.expect("reading a line should always suceed");
        let (city, temperature) = parse_temperature_line(line.as_str());

        station_stats
            .entry(city)
            .and_modify(|stats| {
                stats.count = stats.count + 1;
                stats.sum = stats.sum + temperature;
                stats.max = f32::max(stats.max, temperature);
                stats.min = f32::min(stats.min, temperature);
            })
            .or_insert(StationStats {
                min: temperature,
                sum: temperature,
                max: temperature,
                count: 1,
            });
    }

    if let Some(out_file) = out_file {
        let mut out_file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(false)
            .open(out_file)
            .expect("should be able to open out file for writing results");
        print_results(station_stats, &mut out_file);
    } else {
        let mut out_file = std::io::stdout();
        print_results(station_stats, &mut out_file);
    };

    drop(program_span_guard);
}
