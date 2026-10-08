//! Builds PinMAME's static library from the `vendor/pinmame` submodule, then compiles the
//! C shim against the very same headers and preprocessor definitions, and links both.
//!
//! `cmake/libpinmame/CMakeLists.txt` expects to sit at the root of the PinMAME tree
//! (PinMAME's own CI copies it there). Rather than writing into the submodule, a patched
//! copy is generated in `OUT_DIR` with every tree-relative path made absolute. Each patch
//! must match exactly once (or at least once where stated): a PinMAME update that moves
//! things around fails here, loudly, instead of producing a subtly different library.
//!
//! The shim must see PinMAME's structures with the exact layout the library was built
//! with, so the generated CMake project also writes the target's final compile
//! definitions and include directories to a file that is read back here.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let root = manifest.join("vendor/pinmame");
    let upstream = root.join("cmake/libpinmame/CMakeLists.txt");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=shim/shim.c");
    println!("cargo:rerun-if-changed={}", upstream.display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("src/version.h").display()
    );

    if !upstream.exists() {
        panic!(
            "{} is missing: the PinMAME submodule is not checked out.\n\
             Run `git submodule update --init` (or clone with --recurse-submodules).",
            upstream.display()
        );
    }

    write_sam_sets(&root.join("src/wpc/sam.c"), &out.join("sam_sets.rs"));
    write_sndbrd_names(&root.join("src/wpc/sndbrd.h"), &out.join("sndbrd_names.rs"));

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let msvc = target_env == "msvc";

    let (platform, arch) = match (target_os.as_str(), target_arch.as_str()) {
        ("linux", "x86_64") => ("linux", "x64"),
        ("linux", "aarch64") => ("linux", "aarch64"),
        ("macos", "aarch64") => ("macos", "arm64"),
        ("macos", "x86_64") => ("macos", "x64"),
        ("windows", "x86_64") if msvc => ("win", "x64"),
        ("windows", "x86_64") => ("win-mingw", "x64"),
        (os, a) => panic!("unsupported target {os}/{a}"),
    };

    // CMake wants forward slashes, also on Windows.
    let root_cmake = cmake_path(&root);
    let src_dir = out.join("libpinmame-cmake");
    fs::create_dir_all(&src_dir).unwrap();
    // A Windows checkout has CRLF line endings; the patches match LF.
    let text = fs::read_to_string(&upstream).unwrap().replace("\r\n", "\n");
    fs::write(src_dir.join("CMakeLists.txt"), patch_cmakelists(&text)).unwrap();

    let lib_dir = out.join("lib");
    let lib_dir_cmake = cmake_path(&lib_dir);
    let dst = cmake::Config::new(&src_dir)
        // Always an optimized library: the emulation is the hot path, and on MSVC a Debug
        // CMake build would also pick the debug CRT, which rustc never links.
        .profile("Release")
        .define("PINMAME_ROOT", &root_cmake)
        .define("PLATFORM", platform)
        .define("ARCH", arch)
        .define("BUILD_SHARED", "OFF")
        .define("BUILD_STATIC", "ON")
        // The asmjit JIT only speeds up ARM7 CPUs (Stern SAM, whose sounds are read from the
        // image; the emulation only boots it twice for its volume); without it there is no
        // executable memory to allocate, which a hardened (notarized) macOS binary would
        // need an entitlement for.
        .define("PINMAME_JIT_ASMJIT", "OFF")
        .define("CMAKE_ARCHIVE_OUTPUT_DIRECTORY", &lib_dir_cmake)
        .define("CMAKE_ARCHIVE_OUTPUT_DIRECTORY_RELEASE", &lib_dir_cmake)
        .build_target("pinmame_static")
        .build();

    // Compile definitions + include directories of pinmame_static, as CMake resolved them.
    let flags_file = dst.join("build/rom2altsound_shim_flags_Release.txt");
    let flags = fs::read_to_string(&flags_file)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", flags_file.display()))
        .replace("\r\n", "\n");
    let mut shim = cc::Build::new();
    shim.file("shim/shim.c").warnings(false);
    if !msvc {
        // What CMake uses for C_STANDARD 99 with extensions on (its default).
        shim.flag("-std=gnu99");
    }
    let mut section = "";
    for line in flags.lines() {
        match line {
            "[defines]" | "[includes]" => section = line,
            "" => {}
            l if section == "[defines]" => match l.split_once('=') {
                Some((k, v)) => {
                    shim.define(k, Some(v));
                }
                None => {
                    shim.define(l, None);
                }
            },
            l if section == "[includes]" => {
                shim.include(l);
            }
            l => panic!("unexpected line in {}: {l}", flags_file.display()),
        }
    }
    shim.compile("rom2altsound_shim");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    if platform.starts_with("win") {
        println!("cargo:rustc-link-lib=static=pinmame_static");
        println!("cargo:rustc-link-lib=dylib=winmm");
        // MsgWaitForMultipleObjects, used by cpuexec.c on Windows.
        println!("cargo:rustc-link-lib=dylib=user32");
    } else {
        println!("cargo:rustc-link-lib=static=pinmame");
    }
    match target_os.as_str() {
        "macos" => println!("cargo:rustc-link-lib=dylib=c++"),
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=m");
        }
        _ if !msvc => println!("cargo:rustc-link-lib=dylib=stdc++"),
        _ => {}
    }
}

/// Makes the upstream CMakeLists usable from outside the PinMAME tree (see the module doc).
fn patch_cmakelists(text: &str) -> String {
    let mut t = text.to_owned();
    replace_once(
        &mut t,
        "file(READ src/version.h version)",
        "file(READ ${PINMAME_ROOT}/src/version.h version)",
    );
    // No LTO: GCC would fill the archive with LTO bytecode (and MSVC with /GL objects) that
    // only its own linker understands, while rustc links plain objects.
    replace_once(
        &mut t,
        "check_ipo_supported(RESULT IPO_SUPPORTED OUTPUT IPO_MESSAGE LANGUAGES C CXX)",
        "set(IPO_SUPPORTED FALSE)\nset(IPO_MESSAGE \"disabled by rom2altsound\")",
    );
    replace_once(
        &mut t,
        "      \"$<$<CONFIG:RELEASE>:$<$<COMPILE_LANGUAGE:C,CXX>:/GL>>\"\n",
        "",
    );
    // PinMAME compiles its own zlib (ext/zlib) on every platform, but only adds its headers
    // to the include path on Windows: elsewhere the build picked the system's zlib.h, and
    // failed on a Linux host without the zlib development package. The vendored headers
    // go on every platform, so that they always match the vendored sources.
    replace_once(
        &mut t,
        "   src/unix/sysdep\n)\n",
        "   src/unix/sysdep\n   ext/zlib\n)\n",
    );
    replace_all(
        &mut t,
        "include(${CMAKE_SOURCE_DIR}/cmake/",
        "include(${PINMAME_ROOT}/cmake/",
        2,
    );

    // Everything before `if(BUILD_SHARED)` builds the source and include lists; make them
    // absolute there. After it, only the test program and the install rules use paths.
    let split = t
        .find("\nif(BUILD_SHARED)")
        .expect("`if(BUILD_SHARED)` not found in libpinmame's CMakeLists.txt");
    let (head, tail) = t.split_at(split);
    let mut tail = tail.to_owned();
    let n = tail.matches(" src/libpinmame/").count();
    assert!(
        n >= 2,
        "expected src/libpinmame/ paths after if(BUILD_SHARED)"
    );
    tail = tail.replace(" src/libpinmame/", " ${PINMAME_ROOT}/src/libpinmame/");

    format!(
        "{head}\n\
         list(TRANSFORM PINMAME_SOURCES PREPEND \"${{PINMAME_ROOT}}/\")\n\
         list(TRANSFORM PINMAME_INCLUDE_DIRS PREPEND \"${{PINMAME_ROOT}}/\")\n\
         {tail}\n\
         # rom2altsound: hand the final definitions and include directories to build.rs.\n\
         file(GENERATE OUTPUT \"${{CMAKE_BINARY_DIR}}/rom2altsound_shim_flags_$<CONFIG>.txt\" CONTENT \
         \"[defines]\\n$<JOIN:$<TARGET_PROPERTY:pinmame_static,COMPILE_DEFINITIONS>,\\n>\\n\
         [includes]\\n$<JOIN:$<TARGET_PROPERTY:pinmame_static,INCLUDE_DIRECTORIES>,\\n>\\n\")\n"
    )
}

fn replace_once(text: &mut String, from: &str, to: &str) {
    let n = text.matches(from).count();
    assert!(
        n == 1,
        "libpinmame CMakeLists.txt: expected exactly one `{from}`, found {n}"
    );
    *text = text.replace(from, to);
}

fn replace_all(text: &mut String, from: &str, to: &str, expected: usize) {
    let n = text.matches(from).count();
    assert!(
        n == expected,
        "libpinmame CMakeLists.txt: expected {expected} `{from}`, found {n}"
    );
    *text = text.replace(from, to);
}

fn cmake_path(p: &Path) -> String {
    p.to_str().expect("non UTF-8 path").replace('\\', "/")
}

/// The Stern SAM sets of PinMAME's driver (`src/wpc/sam.c`), as a Rust table: set name,
/// image file name, CRC32 (0 when not dumped) and length. rom2altsound reads SAM sounds
/// statically from the flash image, so it must know a SAM set before running PinMAME.
fn write_sam_sets(sam_c: &Path, dst: &Path) {
    println!("cargo:rerun-if-changed={}", sam_c.display());
    let text = fs::read(sam_c).unwrap_or_else(|e| panic!("{}: {e}", sam_c.display()));
    let text = String::from_utf8_lossy(&text);
    let mut rows = String::new();
    let mut n = 0;
    for line in text.lines() {
        let l = line.trim_start();
        let Some(rest) = ["SAM1_ROM32MB(", "SAM1_ROM128MB("]
            .iter()
            .find_map(|m| l.strip_prefix(m))
        else {
            continue;
        };
        // set, "file", CRC(xxxxxxxx) SHA1(...) | NO_DUMP, length)
        let mut parts = rest.splitn(3, ',');
        let (Some(set), Some(file), Some(tail)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let set = set.trim();
        let file = file.trim().trim_matches('"');
        let crc = tail
            .split_once("CRC(")
            .and_then(|(_, c)| c.split_once(')'))
            .and_then(|(c, _)| u32::from_str_radix(c.trim(), 16).ok())
            .unwrap_or(0);
        let len = tail
            .rsplit_once(',')
            .map(|(_, l)| l.trim().trim_end_matches(';').trim_end_matches(')').trim())
            .and_then(|l| {
                u32::from_str_radix(l.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
            })
            .unwrap_or(0);
        rows.push_str(&format!(
            "    ({set:?}, {file:?}, 0x{crc:08x}, 0x{len:08x}),\n"
        ));
        n += 1;
    }
    assert!(
        n > 100,
        "{}: only {n} SAM sets found, the driver's format changed",
        sam_c.display()
    );
    fs::write(
        dst,
        format!(
            "/// Generated by build.rs from PinMAME's src/wpc/sam.c: (set, image file, CRC32, length).\n\
             pub const SAM_SETS: &[(&str, &str, u32, u32)] = &[\n{rows}];\n"
        ),
    )
    .unwrap();
}

/// The names of PinMAME's sound board types (`#define SNDBRD_X SNDBRD_TYPE(main, sub)` in
/// `src/wpc/sndbrd.h`), as a Rust table: (type value, name). Only the names: which board a
/// game has is read from the linked library at run time. Aliases (`SNDBRD_BY50 SNDBRD_BY32`)
/// are left out, so each value has its first name.
fn write_sndbrd_names(sndbrd_h: &Path, dst: &Path) {
    println!("cargo:rerun-if-changed={}", sndbrd_h.display());
    let text = fs::read_to_string(sndbrd_h)
        .unwrap_or_else(|e| panic!("{}: {e}", sndbrd_h.display()))
        .replace("\r\n", "\n");
    // "4|2" or "(8|4|2)" or "12"
    let eval = |e: &str| -> Option<u32> {
        e.trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split('|')
            .map(|t| t.trim().parse::<u32>().ok())
            .try_fold(0u32, |acc, v| Some(acc | v?))
    };
    let mut rows = String::new();
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("#define SNDBRD_") else {
            continue;
        };
        let mut it = rest.splitn(2, char::is_whitespace);
        let (Some(name), Some(value)) = (it.next(), it.next()) else {
            continue;
        };
        let value = value.split("//").next().unwrap().trim();
        let Some(args) = value
            .strip_prefix("SNDBRD_TYPE(")
            .and_then(|v| v.strip_suffix(')'))
        else {
            continue;
        };
        let Some((main, sub)) = args.split_once(',') else {
            continue;
        };
        let (Some(main), Some(sub)) = (eval(main), eval(sub)) else {
            panic!("{}: cannot read `{line}`", sndbrd_h.display());
        };
        let v = (main << 8) | sub;
        if seen.insert(v) {
            rows.push_str(&format!("    (0x{v:04x}, \"SNDBRD_{name}\"),\n"));
        }
    }
    assert!(
        seen.len() > 50,
        "{}: only {} sound board types found, the header's format changed",
        sndbrd_h.display(),
        seen.len()
    );
    fs::write(
        dst,
        format!(
            "/// Generated by build.rs from PinMAME's src/wpc/sndbrd.h: (SNDBRD_TYPE value, name).\n\
             pub const SNDBRD_NAMES: &[(u32, &str)] = &[\n{rows}];\n"
        ),
    )
    .unwrap();
}
