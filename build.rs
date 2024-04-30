use anyhow::Result;
use std::{env, path::Path, process::Command};
use vergen::EmitBuilder;

// rustdoc-stripper-ignore-next
/// Call to run `glib-compile-resources` to generate compiled gresources to embed
/// in binary with [`gio::resources_register_include`]. `target` is relative to `OUT_DIR`.
///
/// ```no_run
/// glib_build_tools::compile_resources(
///     &["resources"],
///     "resources/resources.gresource.xml",
///     "compiled.gresource",
/// );
/// ```
pub fn compile_resources<P: AsRef<Path>>(source_dirs: &[P], gresource: &str, target: &str) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);

    let mut command = Command::new("glib-compile-resources");

    for source_dir in source_dirs {
        command.arg("--sourcedir").arg(source_dir.as_ref());
    }

    let output = command
        .arg("--target")
        .arg(out_dir.join(target))
        .arg(gresource)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "glib-compile-resources failed with exit status {} and stderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    println!("cargo:rerun-if-changed={gresource}");
    let mut command = Command::new("glib-compile-resources");

    for source_dir in source_dirs {
        command.arg("--sourcedir").arg(source_dir.as_ref());
    }

    let output = command
        .arg("--generate-dependencies")
        .arg(gresource)
        .output()
        .unwrap()
        .stdout;
    let output = String::from_utf8(output).unwrap();
    for dep in output.split_whitespace() {
        println!("cargo:rerun-if-changed={dep}");
    }
}

fn main() -> Result<()>{
    compile_resources(
        &["data"],
        "data/resources.gresource.xml",
        "resources.gresource",
    );
    compile_resources(
        &["data"],
        "data/icons.gresource.xml",
        "icons.gresource",
    );

    EmitBuilder::builder()
        .all_build()
        .all_git()
        .all_sysinfo()
        .fail_on_error()
        .emit()?;
    Ok(())
}
