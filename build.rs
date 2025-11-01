use std::env::{self, var};
use std::fs::{File, write};
use std::io::{BufReader, Write};
use std::path::PathBuf;

use chrono::Timelike;
use quote::quote;
use subjective::Subjective;
use subjective::school::bells::BellTime;

fn main() {
    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");

    println!("cargo:rerun-if-changed=timetable.subjective");
    let subjective: Subjective =
        serde_json::from_reader(BufReader::new(File::open("timetable.subjective").unwrap()))
            .unwrap();

    let weeks = subjective
        .school
        .bell_times
        .iter()
        .map(|week| {
            let days = week
                .days
                .iter()
                .map(|day| {
                    let bell_times = day
                        .iter()
                        .map(|BellTime { time, .. }| {
                            let (hour, minute, second) = (time.hour(), time.minute(), time.second());
                            quote! {
                                BellTime {
                                    time: const { NaiveTime::from_hms_opt(#hour, #minute, #second).unwrap() },
                                    bell_data: None,
                                    enabled: true,
                                }
                            }
                        })
                        .collect::<Vec<_>>();
                    quote! { &[ #(#bell_times),* ] }
                })
                .collect::<Vec<_>>();

            quote! {
                Week {
                    days: &[ #(#days),* ]
                }
            }
        })
        .collect::<Vec<_>>();

    write(
        PathBuf::from(var("OUT_DIR").unwrap()).join("timetable.rs"),
        quote! {
            Subjective {
                school: School {
                    bell_times: &[ #(#weeks),* ],
                },
            }
        }
        .to_string(),
    )
    .unwrap();
}
