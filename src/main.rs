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

const DEFAULT_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const CHANNEL_SIZE: usize = 2 * 1024 * 1024;
const DEFAULT_NUM_THREADS: usize = 3;

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

    for result in results {
        write!(
            &mut out_fd,
            "{}={}/{}/{}\n",
            result.0,
            result.1.min,
            result.1.sum / (result.1.count as f32),
            result.1.max,
        )
        .expect("write to output file should suceed");
    }
}

#[derive(Clone)]
pub struct StationStats {
    pub min: f32,
    pub sum: f32,
    pub max: f32,
    pub count: u64,
}

fn merge_results(
    result: &mut HashMap<String, StationStats>,
    partial_results: &HashMap<String, StationStats>,
) {
    for entry in partial_results.iter() {
        let city = entry.0;
        let station_stats = entry.1;

        result
            .entry(city.clone())
            .and_modify(|stats| {
                stats.count = stats.count + station_stats.count;
                stats.sum = stats.sum + station_stats.sum;
                stats.max = f32::max(stats.max, station_stats.max);
                stats.min = f32::min(stats.min, station_stats.min);
            })
            .or_insert(station_stats.clone());
    }
    return;
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let data_file = args
        .get(1)
        .expect("data file should be passed as an argument")
        .clone();
    let out_file = args.get(2);
    let num_threads = DEFAULT_NUM_THREADS;
    // let num_threads = std::thread::available_parallelism()
    //     .map(|s| s.get() - 4)
    //     .unwrap_or(DEFAULT_NUM_THREADS);

    println!(
        "Running 1BRC on file {} with {} threads",
        data_file, num_threads
    );
    fmt::fmt()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(false)
        .with_level(false)
        .init();
    let program_span = span!(Level::INFO, "program");
    let program_span_guard = program_span.enter();

    let (sender, receiver) = crossbeam_channel::bounded::<String>(CHANNEL_SIZE);
    let mut station_stats: HashMap<String, StationStats> = HashMap::new();

    std::thread::spawn(move || {
        let file = File::open(data_file).expect("should be able to open file for reading");
        let mut reader = BufReader::with_capacity(DEFAULT_BUFFER_SIZE, file);

        let mut line = String::new();
        loop {
            let line_len = reader
                .read_line(&mut line)
                .expect("reading a line should always succeed");
            if line_len == 0 {
                drop(sender);
                break;
            }
            sender
                .send((line.as_str()[0..line_len - 1]).to_string())
                .expect("send should succeed");
            line.clear();
        }
    });

    let mut threads = Vec::with_capacity(num_threads);
    for _i in 0..num_threads {
        let rx = receiver.clone();
        let handle = std::thread::spawn(move || {
            let mut station_stats: HashMap<String, StationStats> = HashMap::new();
            for line in rx.iter() {
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

            return station_stats;
        });
        threads.push(handle);
    }

    for handle in threads {
        let partial_station_stats = handle.join().expect("Thread panicked");
        merge_results(&mut station_stats, &partial_station_stats);
    }

    if let Some(out_file) = out_file {
        let mut out_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(out_file)
            .expect("should be able to open out file for writing results");
        print_results(station_stats, &mut out_file);
    } else {
        let mut out_file = std::io::stdout();
        print_results(station_stats, &mut out_file);
    };

    drop(program_span_guard);
}
