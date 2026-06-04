use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FFMPEG_LIBS: &[&str] = &["avformat", "avcodec", "swresample", "avutil"];

const FFMPEG_SUBDIR: &str = "ffmpeg-8.1.1";
const FFMPEG_URL: &str = "https://ffmpeg.org/releases/ffmpeg-8.1.1.tar.xz";
const FFMPEG_SHA256: &str = "b6863adde98898f42602017462871b5f6333e65aec803fdd7a6308639c52edf3";

const FFMPEG_CONFIGURE_FLAGS: &[&str] = &[
    "--disable-autodetect",
    "--disable-avdevice",
    "--disable-avfilter",
    "--disable-bsfs",
    "--disable-devices",
    "--disable-doc",
    "--disable-everything",
    "--disable-filters",
    "--disable-htmlpages",
    "--disable-manpages",
    "--disable-network",
    "--disable-podpages",
    "--disable-programs",
    "--disable-protocols",
    "--disable-shared",
    "--disable-swscale",
    "--disable-txtpages",
    "--enable-pic",
    "--enable-static",
    "--enable-protocol=file",
    "--enable-libmp3lame",
    "--enable-libvorbis",
    "--enable-libopus",
    "--pkg-config-flags=--static",
];

const ENABLED_DEMUXERS: &[&str] = &[
    "aac", "ac3", "aiff", "amr", "ape", "asf", "au", "bink", "binka", "caf", "dsf", "dts", "dtshd",
    "eac3", "flac", "gsm", "iamf", "iff", "iss", "m4a", "matroska", "mlp", "mov", "mp3", "mp4",
    "mpc", "mpc8", "ogg", "oma", "rm", "shorten", "spdif", "swf", "tak", "truehd", "tta", "voc",
    "w64", "wav", "wv", "xa", "xwma",
];
const ENABLED_MUXERS: &[&str] = &["aiff", "flac", "ipod", "mp3", "ogg", "wav"];

const ENABLED_PARSERS: &[&str] = &[
    "aac_latm",
    "aac",
    "ac3",
    "amr",
    "cook",
    "dca",
    "dolby_e",
    "dvaudio",
    "flac",
    "ftr",
    "g723_1",
    "g729",
    "gsm",
    "misc4",
    "mlp",
    "mpegaudio",
    "opus",
    "sbc",
    "sipr",
    "tak",
    "vorbis",
    "xma",
];

const ENABLED_DECODERS: &[&str] = &[
    "8svx_exp",
    "8svx_fib",
    "aac_latm",
    "aac",
    "ac3",
    "acelp_kelvin",
    "adpcm_4xm",
    "adpcm_adx",
    "adpcm_afc",
    "adpcm_agm",
    "adpcm_aica",
    "adpcm_argo",
    "adpcm_ct",
    "adpcm_dtk",
    "adpcm_ea_maxis_xa",
    "adpcm_ea_r1",
    "adpcm_ea_r2",
    "adpcm_ea_r3",
    "adpcm_ea_xas",
    "adpcm_ea",
    "adpcm_g722",
    "adpcm_g726",
    "adpcm_g726le",
    "adpcm_ima_acorn",
    "adpcm_ima_alp",
    "adpcm_ima_amv",
    "adpcm_ima_apc",
    "adpcm_ima_apm",
    "adpcm_ima_cunning",
    "adpcm_ima_dat4",
    "adpcm_ima_dk3",
    "adpcm_ima_dk4",
    "adpcm_ima_ea_eacs",
    "adpcm_ima_ea_sead",
    "adpcm_ima_iss",
    "adpcm_ima_moflex",
    "adpcm_ima_mtf",
    "adpcm_ima_oki",
    "adpcm_ima_qt",
    "adpcm_ima_rad",
    "adpcm_ima_smjpeg",
    "adpcm_ima_ssi",
    "adpcm_ima_wav",
    "adpcm_ima_ws",
    "adpcm_ima_xbox",
    "adpcm_ms",
    "adpcm_mtaf",
    "adpcm_psx",
    "adpcm_sanyo",
    "adpcm_sbpro_2",
    "adpcm_sbpro_3",
    "adpcm_sbpro_4",
    "adpcm_swf",
    "adpcm_thp_le",
    "adpcm_thp",
    "adpcm_vima",
    "adpcm_xa",
    "adpcm_xmd",
    "adpcm_yamaha",
    "adpcm_zork",
    "alac",
    "als",
    "amrnb",
    "amrwb",
    "ape",
    "atrac1",
    "atrac3",
    "atrac3al",
    "atrac3p",
    "atrac3pal",
    "atrac9",
    "binkaudio_dct",
    "binkaudio_rdft",
    "bmv_audio",
    "bonk",
    "cbd2_dpcm",
    "comfortnoise",
    "cook",
    "dca",
    "derf_dpcm",
    "dolby_e",
    "dsd_lsbf_planar",
    "dsd_lsbf",
    "dsd_msbf_planar",
    "dsd_msbf",
    "dsicinaudio",
    "dst",
    "dvaudio",
    "eac3",
    "evrc",
    "fastaudio",
    "flac",
    "ftr",
    "g723_1",
    "g728",
    "g729",
    "gremlin_dpcm",
    "gsm_ms",
    "gsm",
    "hca",
    "hcom",
    "iac",
    "ilbc",
    "imc",
    "interplay_acm",
    "interplay_dpcm",
    "mace3",
    "mace6",
    "metasound",
    "misc4",
    "mlp",
    "mp1",
    "mp1float",
    "mp2",
    "mp2float",
    "mp3",
    "mp3adu",
    "mp3adufloat",
    "mp3float",
    "mp3on4",
    "mp3on4float",
    "mpc7",
    "mpc8",
    "msnsiren",
    "nellymoser",
    "on2avc",
    "opus",
    "osq",
    "paf_audio",
    "pcm_alaw",
    "pcm_bluray",
    "pcm_dvd",
    "pcm_f16le",
    "pcm_f24le",
    "pcm_f32be",
    "pcm_f32le",
    "pcm_f64be",
    "pcm_f64le",
    "pcm_lxf",
    "pcm_mulaw",
    "pcm_s16be_planar",
    "pcm_s16be",
    "pcm_s16le_planar",
    "pcm_s16le",
    "pcm_s24be",
    "pcm_s24daud",
    "pcm_s24le_planar",
    "pcm_s24le",
    "pcm_s32be",
    "pcm_s32le_planar",
    "pcm_s32le",
    "pcm_s64be",
    "pcm_s64le",
    "pcm_s8_planar",
    "pcm_s8",
    "pcm_sga",
    "pcm_u16be",
    "pcm_u16le",
    "pcm_u24be",
    "pcm_u24le",
    "pcm_u32be",
    "pcm_u32le",
    "pcm_u8",
    "pcm_vidc",
    "qcelp",
    "qdm2",
    "qdmc",
    "qoa",
    "ra_144",
    "ra_288",
    "ralf",
    "rka",
    "roq_dpcm",
    "s302m",
    "sbc",
    "sdx2_dpcm",
    "shorten",
    "sipr",
    "siren",
    "smackaud",
    "sol_dpcm",
    "sonic",
    "speex",
    "tak",
    "truehd",
    "truespeech",
    "tta",
    "twinvq",
    "vmdaudio",
    "vorbis",
    "wady_dpcm",
    "wavarc",
    "wavpack",
    "wmalossless",
    "wmapro",
    "wmav1",
    "wmav2",
    "wmavoice",
    "ws_snd1",
    "xan_dpcm",
    "xma1",
    "xma2",
];
const ENABLED_ENCODERS: &[&str] = &[
    "aac",
    "alac",
    "flac",
    "libmp3lame",
    "libopus",
    "libvorbis",
    "pcm_f32be",
    "pcm_f32le",
    "pcm_f64be",
    "pcm_f64le",
    "pcm_s16be",
    "pcm_s16le",
    "pcm_s24be",
    "pcm_s24le",
    "pcm_s32be",
    "pcm_s32le",
    "pcm_s8",
    "pcm_u8",
];

const THIRD_PARTY_LINK_LIBS: &[&str] = &["mp3lame", "opus", "vorbisenc", "vorbis", "ogg"];

/// Prevent any autotools from regenerating build files
const AUTOTOOLS_REGEN_NOOPS: &[&str] = &[
    "ACLOCAL=true",
    "AUTOCONF=true",
    "AUTOMAKE=true",
    "AUTOHEADER=true",
    "MAKEINFO=true",
];

struct VendoredDep {
    subdir: &'static str,
    url: &'static str,
    sha256: &'static str,
    build_name: &'static str,
    configure_args: &'static [&'static str],
    needs_pkgconfig: bool,
}

const THIRD_PARTY_DEPS: &[VendoredDep] = &[
    VendoredDep {
        subdir: "lame-3.100",
        url: "https://downloads.sourceforge.net/project/lame/lame/3.100/lame-3.100.tar.gz",
        sha256: "ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e",
        build_name: "lame",
        configure_args: &[
            "--disable-shared",
            "--enable-static",
            "--disable-frontend",
            "--disable-decoder",
            "--with-pic",
        ],
        needs_pkgconfig: false,
    },
    VendoredDep {
        subdir: "libogg-1.3.5",
        url: "https://downloads.xiph.org/releases/ogg/libogg-1.3.5.tar.xz",
        sha256: "c4d91be36fc8e54deae7575241e03f4211eb102afb3fc0775fbbc1b740016705",
        build_name: "libogg",
        configure_args: &["--disable-shared", "--enable-static", "--with-pic"],
        needs_pkgconfig: false,
    },
    VendoredDep {
        subdir: "libvorbis-1.3.7",
        url: "https://downloads.xiph.org/releases/vorbis/libvorbis-1.3.7.tar.xz",
        sha256: "b33cc4934322bcbf6efcbacf49e3ca01aadbea4114ec9589d1b1e9d20f72954b",
        build_name: "libvorbis",
        configure_args: &[
            "--disable-shared",
            "--enable-static",
            "--disable-oggtest",
            "--disable-examples",
            "--disable-docs",
            "--with-pic",
        ],
        needs_pkgconfig: true,
    },
    VendoredDep {
        subdir: "opus-1.5.2",
        url: "https://downloads.xiph.org/releases/opus/opus-1.5.2.tar.gz",
        sha256: "65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1",
        build_name: "opus",
        configure_args: &[
            "--disable-shared",
            "--enable-static",
            "--disable-doc",
            "--disable-extra-programs",
            "--with-pic",
        ],
        needs_pkgconfig: false,
    },
];

fn source_root(out: &Path) -> PathBuf {
    match env::var_os("CODECPOD_VENDOR_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => out.join("sources"),
    }
}

fn ensure_source(out: &Path, subdir: &str, url: &str, sha256: &str) -> PathBuf {
    let root = source_root(out);
    let dest = root.join(subdir);

    if env::var_os("CODECPOD_VENDOR_DIR").is_some() {
        assert!(
            dest.is_dir(),
            "CODECPOD_VENDOR_DIR set to {}, but expected subdir {} not found",
            root.display(),
            dest.display()
        );
        return dest;
    }

    if dest.join(".extracted").is_file() {
        return dest;
    }

    fs::create_dir_all(&root).expect("create sources root");

    let filename = url.rsplit('/').next().expect("url has filename");
    let bytes = download(url);

    let actual = sha256_hex(&bytes);
    assert_eq!(actual, sha256, "SHA256 mismatch for {filename}");

    extract(&bytes, filename, &root);

    assert!(
        dest.is_dir(),
        "extracting {filename} did not produce expected subdir {}",
        dest.display()
    );
    fs::write(dest.join(".extracted"), sha256).expect("write extracted marker");

    dest
}

fn download(url: &str) -> Vec<u8> {
    const MAX_BODY: u64 = 256 * 1024 * 1024;
    const ATTEMPTS: u32 = 3;

    let mut last_err = String::new();
    for attempt in 1..=ATTEMPTS {
        match ureq::get(url).call() {
            Ok(mut resp) => match resp.body_mut().with_config().limit(MAX_BODY).read_to_vec() {
                Ok(bytes) => return bytes,
                Err(e) => last_err = format!("reading body: {e}"),
            },
            Err(e) => last_err = format!("request: {e}"),
        }
        eprintln!("download attempt {attempt}/{ATTEMPTS} for {url} failed: {last_err}");
    }
    panic!("failed to download {url} after {ATTEMPTS} attempts: {last_err}");
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;

    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        write!(hex, "{byte:02x}").expect("write to String");
    }
    hex
}

fn extract(tarball: &[u8], filename: &str, dest_root: &Path) {
    if filename.ends_with(".tar.xz") {
        let mut decompressed = Vec::new();
        let mut input = tarball;
        lzma_rs::xz_decompress(&mut input, &mut decompressed)
            .unwrap_or_else(|e| panic!("xz decompression of {filename} failed: {e}"));
        tar::Archive::new(decompressed.as_slice())
            .unpack(dest_root)
            .unwrap_or_else(|e| panic!("untarring {filename} failed: {e}"));
    } else if filename.ends_with(".tar.gz") {
        let decoder = flate2::read::GzDecoder::new(tarball);
        tar::Archive::new(decoder)
            .unpack(dest_root)
            .unwrap_or_else(|e| panic!("untarring {filename} failed: {e}"));
    } else {
        panic!("unsupported archive format: {filename}");
    }
}

fn out_dir() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap())
}

fn target_is_windows() -> bool {
    env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
}

fn to_mingw_path(p: &Path) -> String {
    let output = Command::new("cygpath")
        .arg("-m")
        .arg(p)
        .output()
        .expect("spawn cygpath");
    assert!(
        output.status.success(),
        "cygpath -m failed for {}",
        p.display()
    );
    String::from_utf8(output.stdout)
        .expect("cygpath output is utf8")
        .trim()
        .to_string()
}

fn pkgconfig_path(prefix: &Path) -> OsString {
    let dir = prefix.join("lib").join("pkgconfig");
    if target_is_windows() {
        OsString::from(to_mingw_path(&dir))
    } else {
        dir.into_os_string()
    }
}

fn run_autotools(
    src: &Path,
    build: &Path,
    prefix: &Path,
    configure_args: &[String],
    make_targets: &[String],
    install: bool,
    env_vars: &[(&str, OsString)],
) {
    let windows = target_is_windows();

    let build_dir = if windows { src } else { build };
    if !windows {
        fs::create_dir_all(build).expect("create autotools build dir");
    }

    let mut configure = if windows {
        // Relative "configure" with cwd == srcdir makes autoconf set srcdir to ".", so no absolute
        // path is ever handed to the MinGW tools.
        let mut c = Command::new("sh");
        c.arg("configure");
        c
    } else {
        Command::new(src.join("configure"))
    };
    let prefix_arg = if windows {
        to_mingw_path(prefix)
    } else {
        prefix.display().to_string()
    };
    configure
        .arg(format!("--prefix={prefix_arg}"))
        .args(configure_args)
        .current_dir(build_dir);
    if windows {
        configure.env("MSYS2_ARG_CONV_EXCL", "*");
    }
    for (k, v) in env_vars {
        configure.env(k, v);
    }
    let status = configure.status().expect("spawn configure");
    assert!(
        status.success(),
        "configure for {} exited with {status}",
        src.display()
    );

    let nproc = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let mut make = Command::new("make");
    make.arg(format!("-j{nproc}"))
        .args(AUTOTOOLS_REGEN_NOOPS)
        .args(make_targets)
        .current_dir(build_dir);
    if windows {
        make.env("MSYS2_ARG_CONV_EXCL", "*");
    }
    let status = make.status().expect("spawn make");
    assert!(
        status.success(),
        "make for {} exited with {status}",
        src.display()
    );

    if install {
        let mut make_install = Command::new("make");
        make_install
            .arg("install")
            .args(AUTOTOOLS_REGEN_NOOPS)
            .current_dir(build_dir);
        if windows {
            make_install.env("MSYS2_ARG_CONV_EXCL", "*");
        }
        let status = make_install.status().expect("spawn make install");
        assert!(
            status.success(),
            "make install for {} exited with {status}",
            src.display()
        );
    }
}

fn emit_link_flags(ffmpeg_build: &Path, deps_prefix: &Path) {
    for lib in FFMPEG_LIBS {
        println!(
            "cargo:rustc-link-search=native={}",
            ffmpeg_build.join(format!("lib{lib}")).display()
        );
        println!("cargo:rustc-link-lib=static={lib}");
    }

    println!(
        "cargo:rustc-link-search=native={}",
        deps_prefix.join("lib").display()
    );
    for lib in THIRD_PARTY_LINK_LIBS {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => {}
        "windows" => println!("cargo:rustc-link-lib=dylib=bcrypt"),
        _ => {
            println!("cargo:rustc-link-lib=m");
            println!("cargo:rustc-link-lib=pthread");
        }
    }
}

fn run_bindgen(src: &Path, build: &Path) {
    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", src.display()))
        .clang_arg(format!("-I{}", build.display()));
    if target_is_windows() {
        // bindgen feeds clang the Rust target triple verbatim, but clang doesn't understand the
        // `gnullvm` environment.
        builder = builder.clang_arg("--target=x86_64-pc-windows-gnu");
    }
    let bindings = builder
        .allowlist_function("av_.*")
        .allowlist_function("avformat_.*")
        .allowlist_function("avcodec_.*")
        .allowlist_function("avio_.*")
        .allowlist_function("swr_.*")
        .allowlist_var("AV_.*")
        .allowlist_var("AVERROR.*")
        .allowlist_var("AVFMT_.*")
        .allowlist_var("AVIO_.*")
        .allowlist_var("FF_.*")
        .allowlist_type("AV.*")
        .allowlist_type("Swr.*")
        .layout_tests(false)
        .generate_comments(false)
        .derive_copy(false)
        .derive_debug(false)
        .prepend_enum_name(false)
        .generate()
        .expect("bindgen failed to generate FFmpeg bindings");
    bindings
        .write_to_file(out_dir().join("ffmpeg.rs"))
        .expect("failed to write bindgen output");
}

/// lame / libogg / libvorbis carry a Darwin branch in their configure script that injects the
/// obsolete `-force_cpusubtype_ALL` linker flag, which modern macOS ld rejects. Strip it before
/// configuring. Idempotent: a no-op when the flag is absent.
fn strip_obsolete_darwin_ldflag(src: &Path) {
    let configure = src.join("configure");
    let Ok(content) = fs::read_to_string(&configure) else {
        return;
    };
    if content.contains("-force_cpusubtype_ALL") {
        fs::write(&configure, content.replace("-force_cpusubtype_ALL", ""))
            .expect("patch configure to drop -force_cpusubtype_ALL");
    }
}

/// lame 3.100 enables its SSE path on x86 whenever <xmmintrin.h> compiles, but under MinGW the noinst
/// vector archive (liblamevectorroutines.a) that holds `init_xrpow_core_sse` is never produced, so
/// libmp3lame links with that symbol undefined. The probe is a hand-written AC_COMPILE_IFELSE that
/// overwrites any preset cache var, so patch the generated configure to force it off. Idempotent.
fn disable_lame_sse_intrinsics(src: &Path) {
    let configure = src.join("configure");
    let Ok(content) = fs::read_to_string(&configure) else {
        return;
    };
    let patched = content
        .replace("$as_echo \"#define HAVE_XMMINTRIN_H 1\" >>confdefs.h", "")
        .replace(
            "ac_cv_header_xmmintrin_h=yes",
            "ac_cv_header_xmmintrin_h=no",
        );
    if patched != content {
        fs::write(&configure, patched).expect("patch lame configure to disable SSE intrinsics");
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    if env::var_os("DOCS_RS").is_some() {
        return;
    }

    let out = out_dir();
    let deps_prefix = out.join("deps");
    let third_party_build = out.join("third-party-build");
    let ffmpeg_build = out.join("ffmpeg-build");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=CODECPOD_VENDOR_DIR");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let ffmpeg_src = ensure_source(&out, FFMPEG_SUBDIR, FFMPEG_URL, FFMPEG_SHA256);

    fs::create_dir_all(deps_prefix.join("lib").join("pkgconfig"))
        .expect("create deps pkgconfig dir");

    for dep in THIRD_PARTY_DEPS {
        let env: Vec<(&str, OsString)> = if dep.needs_pkgconfig {
            vec![("PKG_CONFIG_PATH", pkgconfig_path(&deps_prefix))]
        } else {
            vec![]
        };
        let configure_args: Vec<String> =
            dep.configure_args.iter().map(|s| s.to_string()).collect();
        let dep_src = ensure_source(&out, dep.subdir, dep.url, dep.sha256);
        if target_os == "macos" {
            strip_obsolete_darwin_ldflag(&dep_src);
        }
        if target_os == "windows" && dep.build_name == "lame" {
            disable_lame_sse_intrinsics(&dep_src);
        }
        run_autotools(
            &dep_src,
            &third_party_build.join(dep.build_name),
            &deps_prefix,
            &configure_args,
            &[],
            true,
            &env,
        );
    }

    let mut ffmpeg_args: Vec<String> = FFMPEG_CONFIGURE_FLAGS
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Third-party libraries are installed under deps_prefix. libvorbis / libopus ship .pc files,
    // so FFmpeg can locate them via PKG_CONFIG_PATH. LAME, however, has no pkg-config file; FFmpeg
    // uses check_lib (a compile+link probe) for libmp3lame, so the include / lib directories of
    // deps_prefix must be explicitly fed to FFmpeg's own compiler detection — otherwise configure
    // fails to find lame/lame.h and -lmp3lame, reporting "libmp3lame not found".
    let (inc_dir, lib_dir) = if target_os == "windows" {
        (
            to_mingw_path(&deps_prefix.join("include")),
            to_mingw_path(&deps_prefix.join("lib")),
        )
    } else {
        (
            deps_prefix.join("include").display().to_string(),
            deps_prefix.join("lib").display().to_string(),
        )
    };
    ffmpeg_args.push(format!("--extra-cflags=-I{inc_dir}"));
    ffmpeg_args.push(format!("--extra-ldflags=-L{lib_dir}"));
    ffmpeg_args.push(format!("--enable-demuxer={}", ENABLED_DEMUXERS.join(",")));
    ffmpeg_args.push(format!("--enable-parser={}", ENABLED_PARSERS.join(",")));
    ffmpeg_args.push(format!("--enable-decoder={}", ENABLED_DECODERS.join(",")));
    ffmpeg_args.push(format!("--enable-muxer={}", ENABLED_MUXERS.join(",")));
    ffmpeg_args.push(format!("--enable-encoder={}", ENABLED_ENCODERS.join(",")));
    if target_os == "windows" {
        ffmpeg_args.push(format!(
            "--tempprefix={}/ffconf",
            to_mingw_path(&ffmpeg_src)
        ));
    }

    let ffmpeg_targets: Vec<String> = FFMPEG_LIBS
        .iter()
        .map(|lib| format!("lib{lib}/lib{lib}.a"))
        .collect();

    run_autotools(
        &ffmpeg_src,
        &ffmpeg_build,
        &deps_prefix,
        &ffmpeg_args,
        &ffmpeg_targets,
        false,
        &[("PKG_CONFIG_PATH", pkgconfig_path(&deps_prefix))],
    );

    let ffmpeg_out = if target_os == "windows" {
        &ffmpeg_src
    } else {
        &ffmpeg_build
    };
    emit_link_flags(ffmpeg_out, &deps_prefix);
    run_bindgen(&ffmpeg_src, ffmpeg_out);
}
