// Decodes FM audio from 12kHz ICOM IF

use clap::{Parser, ColorChoice};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{traits::*, HeapRb};
use sdr::FIR;
use std::thread;
use console::{Key, Term};

#[derive(Parser, Debug)]
#[command(version, about = "FM audio decoder from 12kHz ICOM IF by R2AJP", long_about = None, color = ColorChoice::Never)]
struct Opt {
    // The input audio device to use
    #[arg(short, long, value_name = "IN", default_value_t = String::from("CABLE-A Output (VB-Audio Cable A)"))]
    input_device: String,

    // The output audio device to use
    #[arg(short, long, value_name = "OUT", default_value_t = String::from("CABLE-B Input (VB-Audio Cable B)"))]
    output_device: String,

    // Use the JACK host
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    #[arg(short, long)]
    #[allow(dead_code)]
    jack: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();
    println!("FM audio decoder from 12kHz ICOM IF by R2AJP");

    // Conditionally compile with jack if the feature is specified.
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    // Manually check for flags. Can be passed through cargo with -- e.g.
    // cargo run --release --example beep --features jack -- --jack
    let host = if opt.jack {
        cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .expect(
                "make sure --features jack is specified. only works on OSes where jack is available",
            )).expect("jack host unavailable")
    } else {
        cpal::default_host()
    };

    #[cfg(any(
        not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        )),
        not(feature = "jack")
    ))]
    let host = cpal::default_host();

    // Find devices.
    let input_device = if opt.input_device == "default" {
        host.default_input_device()
    } else {
        host.input_devices()?
            .find(|x| x.name().map(|y| y == opt.input_device).unwrap_or(false))
    }
    .expect("failed to find input device");

    let output_device = if opt.output_device == "default" {
        host.default_output_device()
    } else {
        host.output_devices()?
            .find(|x| x.name().map(|y| y == opt.output_device).unwrap_or(false))
    }
    .expect("failed to find output device");

    println!("Using input device: \"{}\"", input_device.name()?);
    println!("Using output device: \"{}\"", output_device.name()?);

    // We'll try and use the same configuration between streams to keep it simple.
    // It must be 2 channels, 48 ksamples, f32
    let config: cpal::StreamConfig = input_device.default_input_config()?.into();
    let output_channels = config.channels as usize;

    // Create a 2 * 300ms delay in case the input and output devices aren't synced.
    let latency_frames = (300.0 / 1_000.0) * config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize * config.channels as usize;

    // The buffer to transfer samples to processing thread
    let ring = HeapRb::<f32>::new(latency_samples * 2);
    let (mut producer, mut consumer) = ring.split();

    // The buffer to transfer samples from processing thread
    let ring2 = HeapRb::<f32>::new(latency_samples * 2);
    let (mut producer2, mut consumer2) = ring2.split();

    // Fill the samples with 0.0 equal to the length of the delay.
    for _ in 0..latency_samples {
        producer.try_push(0.0).unwrap();
        producer2.try_push(0.0).unwrap();
    }

    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut output_fell_behind = false;

        let mut channel_counter = 0u8;
        for &sample in data {
            // take samples only from one channel - stream must be stereo!!!
            if channel_counter == 0 {
                if producer.try_push(sample).is_err() {
                    output_fell_behind = true;
                };
                channel_counter += 1;
            } else {
                channel_counter = 0;
            };
        }

        if output_fell_behind {
            eprintln!("output stream fell behind");
        }
    };

    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let mut input_fell_behind = false;

        for frame in data.chunks_mut(output_channels) {
            let value = match consumer2.try_pop() {
                Some(s) => s,
                None => {
                    input_fell_behind = true;
                    0.0
                }
            };
            // set left and right cnannels the same
            for sample in frame.iter_mut() {
                *sample = value;
            }
        }

        if input_fell_behind {
            eprintln!("input stream fell behind");
        }
    };

    // Build streams.
    println!(
        "Attempting to build both streams with f32 samples and `{:?}`.",
        config
    );
    let input_stream = input_device.build_input_stream(&config, input_data_fn, err_fn, None)?;
    let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn, None)?;
    println!("Successfully built streams.");

    // Play the streams.
    println!("Starting streams...");
    input_stream.play()?;
    output_stream.play()?;

    // Start FM radio decoding thread
    let _hnd = thread::spawn(move || {
        println!("FM decoding of ICOM IF 12kHz stream... ");

        // output f32 sample
        let mut x: Vec<f32> = Vec::with_capacity(1);
        x.push(0.0);

        // IQ data
        let mut iq_re1: Vec<f32> = Vec::with_capacity(1);
        iq_re1.push(0.0);
        let mut iq_im1: Vec<f32> = Vec::with_capacity(1);
        iq_im1.push(0.0);

        // IQ FIR filters
        let mut firi = FIR::<f32>::lowpass(89, 0.12);
        let mut firq = FIR::<f32>::lowpass(89, 0.12);

        // Audio output FIR filter
        let mut fir = FIR::<f32>::lowpass(60, 0.25);

        // FM decoder samples
        let mut d0re = 0.01f32;
        let mut d0im = 0.01f32;

        // Fs/4 downconverter step
        let mut quadrant = 0u8;

        loop {
            if consumer.is_empty() {
                // in case of no stream data sleep for a while
                std::thread::sleep(std::time::Duration::from_nanos(10_000));
            } else {
                let sample = match consumer.try_pop() {
                    Some(s) => s,
                    None => 0.0,
                };
                // all SDR processing here!!!
                // downconver Fs/4 as ICOM IF is 12000 Hz
                match quadrant {
                    0 => {
                        iq_re1[0] = sample;
                        iq_im1[0] = 0.0;
                    }
                    1 => {
                        iq_re1[0] = 0.0;
                        iq_im1[0] = -sample;
                    }
                    2 => {
                        iq_re1[0] = -sample;
                        iq_im1[0] = 0.0;
                    }
                    3 => {
                        iq_re1[0] = 0.0;
                        iq_im1[0] = sample;
                    }
                    _ => panic!("Quadrant out of range"),
                }
                // move to next sample
                if quadrant == 3 {
                    quadrant = 0;
                } else {
                    quadrant += 1;
                }

                // filter IQ signals
                let filtered_re = firi.process(&iq_re1);
                let filtered_im = firq.process(&iq_im1);

                // store z-1 sample for FM demod
                let d1re = d0re;
                let d1im = d0im;
                d0re = filtered_re[0];
                d0im = filtered_im[0];

                // FM demod
                // K-mod set to 0.1
                const DECON: f32 = 1.0 / (2.0 * std::f32::consts::PI * 0.1);
                let top = d1re * d0im - d1im * d0re;
                let bottom = d1re * d0re + d1im * d0im;
                if bottom == 0.0 {
                    x[0] = DECON * top.atan();
                } else {
                    x[0] = DECON * ((top / bottom).atan());
                }

                // LPF for output audio
                let filtered = fir.process(&x);

                // send audio sample to output stream
                producer2.try_push(filtered[0]).ok();
            }
        }
    });
    
    // Run for 3 seconds before allowing to exit )
    std::thread::sleep(std::time::Duration::from_secs(3));
    println!("Running... Press Esc to exit");
    let term = Term::stdout();
    loop {
        let key = term.read_key()?;
        if key == Key::Escape {
            break;
        }
    }
    drop(input_stream);
    drop(output_stream);
    println!("Done!");
    Ok(())
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}
