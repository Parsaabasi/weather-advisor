fn main() {
    linker_be_nice();
    pass_through_env();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("riscv32") || target.contains("xtensa") {
        println!("cargo::rustc-link-arg=-Tlinkall.x");
    }
}

// Reads the compile-time configuration from the environment and forwards it to
// rustc, so `env!`/`option_env!` in the crate see it even when the variable
// would not otherwise reach the compiler (e.g. builds launched from an IDE).
// `rerun-if-env-changed` makes cargo re-run this script — and recompile the
// crate — whenever a variable is added or changed, so switching credentials
// never serves a stale binary.
//
// WIFI_SSID / WIFI_PASSWORD are required for the device target (the `env!`
// in src/bin/main.rs fails the build with a clear error when they are absent);
// WEATHER_CITY is optional and defaults to Brno in the code.
fn pass_through_env() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let device_build = target.contains("riscv32") || target.contains("xtensa");

    let mut missing = Vec::new();
    for key in ["WIFI_SSID", "WIFI_PASSWORD", "WEATHER_CITY"] {
        println!("cargo::rerun-if-env-changed={key}");
        match std::env::var(key) {
            Ok(value) => println!("cargo::rustc-env={key}={value}"),
            Err(_) if key != "WEATHER_CITY" => missing.push(key),
            Err(_) => {}
        }
    }

    if device_build && !missing.is_empty() {
        println!(
            "cargo::warning=missing Wi-Fi credentials: {} (set them at build time, e.g. `WIFI_SSID=your-ssid WIFI_PASSWORD=your-password cargo build --release`)",
            missing.join(" and ")
        );
    }
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => {},
                // we don't have anything helpful for "missing-lib" yet
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        println!(
            "cargo::rustc-link-arg=-Wl,--error-handling-script={}",
            std::env::current_exe().unwrap().display()
        );
    }
}
