use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::io::{BufReader, Read};

use tracing::span;
use tracing::Level;
use tracing_subscriber;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FmtSpan;

const DEFAULT_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const CHANNEL_SIZE: usize = 2 * 1024 * 1024;
const DEFAULT_NUM_THREADS: usize = 3;

use memchr::memchr_iter;

fn split_newline_simd(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut results = Vec::new();
    let mut last_pos = 0;

    // memchr_iter finds all occurrences of '\n' (0x0A) using SIMD
    for pos in memchr_iter(b'\n', bytes) {
        results.push(&text[last_pos..pos]);
        last_pos = pos + 1;
    }

    // Push the last remaining part
    if last_pos <= bytes.len() {
        results.push(&text[last_pos..]);
    }

    results
}

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
    let num_threads = std::thread::available_parallelism()
        .map(|s| s.get() - 1)
        .unwrap_or(DEFAULT_NUM_THREADS);

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

        let mut buffer = vec![0u8; DEFAULT_BUFFER_SIZE];
        let mut leftover = Vec::new();

        loop {
            let bytes_read = reader
                .read(&mut buffer)
                .expect("reading should always succeed");

            if bytes_read == 0 {
                // Send any remaining leftover data
                if !leftover.is_empty() {
                    let chunk = String::from_utf8_lossy(&leftover).to_string();
                    sender.send(chunk).expect("send should succeed");
                }
                drop(sender);
                break;
            }

            // Find the last newline in the current buffer
            let last_newline_pos = buffer[0..bytes_read].iter().rposition(|&b| b == b'\n');

            if let Some(pos) = last_newline_pos {
                // Combine leftover from previous read with data up to (but not including) the last newline
                let mut chunk_data = leftover.clone();
                chunk_data.extend_from_slice(&buffer[0..pos]);

                // Convert to string and send
                let chunk = String::from_utf8_lossy(&chunk_data).to_string();
                sender.send(chunk).expect("send should succeed");

                // Save the data after the last newline for the next iteration
                leftover.clear();
                leftover.extend_from_slice(&buffer[pos + 1..bytes_read]);
            } else {
                // No newline found in this chunk, add everything to leftover
                leftover.extend_from_slice(&buffer[0..bytes_read]);
            }
        }
    });

    let mut threads = Vec::with_capacity(num_threads);
    for _i in 0..num_threads {
        let rx = receiver.clone();
        let handle = std::thread::spawn(move || {
            let mut station_stats: HashMap<String, StationStats> = HashMap::new();
            for lines in rx.iter() {
                let lines = split_newline_simd(lines.as_str());
                for line in lines {
                    let (city, temperature) = parse_temperature_line(&line);
                    station_stats
                        .entry(city.clone())
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
