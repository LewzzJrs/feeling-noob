#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use fnoob_core::catalog::{ScanClass, ScanReport};
use fnoob_platform::plugin::{cid_to_hex, scan};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: fnoob-scan <plugin.vst3>");
        return ExitCode::from(2);
    };
    let result = scan(&path);
    let ok = result.is_ok();
    let (classes, error) = match result {
        Ok(classes) => (
            classes
                .into_iter()
                .map(|c| ScanClass {
                    class_id: cid_to_hex(&c.cid),
                    instrument: c.is_instrument(),
                    name: c.name,
                    vendor: c.vendor,
                })
                .collect(),
            None,
        ),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    let report = ScanReport {
        path: path.display().to_string(),
        classes,
        error,
    };
    match serde_json::to_string(&report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("{e}"),
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
