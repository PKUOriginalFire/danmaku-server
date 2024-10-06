use std::{io, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/package.json");
    Command::new("yarn")
        .current_dir("frontend")
        .arg("build")
        .status()
        .and_then(|status| {
            status
                .success()
                .then_some(())
                .ok_or(io::Error::new(io::ErrorKind::Other, "yarn failed"))
        })
        .expect("failed to build frontend");
}
