use std::{
    fs::{File, Metadata, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use hemtt_config::rapify::Derapify;
use hemtt_pbo::ReadablePbo;

use crate::Error;

#[derive(clap::Args)]
pub struct PboUnpackArgs {
    /// PBO file to unpack
    pbo: String,
    /// Directory to unpack to
    ///
    /// If not specified, creates a directory named after the PBO file.
    output: Option<String>,
    #[arg(long = "derap", short = 'r')]
    /// Derapify any rapified files (config.bin, etc.)
    ///
    /// Automatically converts binary config files back to a readable format.
    ///
    /// Derapified files that do not have different file names for the derapified versions will have `.derap` added to their filenames.
    derap: bool,
}

/// Execute the unpack command
///
/// # Errors
/// [`Error`] depending on the modules
pub fn execute(args: &PboUnpackArgs) -> Result<(), Error> {
    let pbo_path = PathBuf::from(&args.pbo);
    let mut pbo = ReadablePbo::from(File::open(&pbo_path)?)?;
    let output = args.output.as_ref().map_or_else(
        || {
            pbo_path
                .file_stem()
                .map_or_else(|| PathBuf::from("unpacked_pbo"), PathBuf::from)
        },
        |output| PathBuf::from(&output),
    );
    // Validate the entire archive before creating even the output directory.
    // PBO paths use Windows separators regardless of the host operating system.
    let files = pbo
        .files()
        .into_iter()
        .map(|header| entry_path(header.filename()).map(|path| (header, path)))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs_err::create_dir_all(parent)?;
    }
    // An exclusive directory creation also rejects existing/dangling links.
    fs_err::create_dir(&output)?;
    let output = fs_err::canonicalize(output)?;
    let mut properties_file = None;
    for (key, value) in pbo.properties() {
        if key == "prefix" {
            let mut file = create_output_file(&output, Path::new("$PBOPREFIX$"))?;
            file.write_all(value.as_bytes())?;
        } else {
            let file = if let Some(ref mut file) = properties_file {
                file
            } else {
                properties_file.insert(create_output_file(&output, Path::new("properties.txt"))?)
            };
            writeln!(file, "{key}={value}")?;
        }
    }
    for (header, path) in files {
        let mut out = create_output_file(&output, &path)?;
        let mut file = pbo
            .file(header.filename())?
            .expect("file must exist if header exists");

        if args.derap {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            let mut cursor = std::io::Cursor::new(buffer);
            let mut header = vec![0; 4];
            let Ok(()) = cursor.read_exact(&mut header) else {
                std::io::copy(&mut cursor, &mut out)?;
                continue;
            };
            cursor.set_position(0);
            if header == b"\0raP" || header == b"BLMX" {
                let file_name = path
                    .file_name()
                    .expect("file must have a name")
                    .to_string_lossy();
                let raw_path = match file_name.as_ref() {
                    "config.bin" => path.with_file_name("config.cpp"),
                    "stringtable.bin" => path.with_file_name("stringtable.xml"),
                    _ => path.with_file_name(format!(
                        "{}.derap.{}",
                        path.file_stem()
                            .expect("file must have a stem")
                            .to_string_lossy(),
                        path.extension()
                            .expect("file must have an extension")
                            .to_string_lossy()
                    )),
                };
                let mut raw_out = create_output_file(&output, &raw_path)?;
                let derap = match file_name.as_ref() {
                    "stringtable.bin" => {
                        let mut writer = String::new();
                        if let Err(e) = hemtt_stringtable::derapify(
                            pbo_path
                                .file_stem()
                                .expect("must have stem")
                                .to_string_lossy()
                                .to_string(),
                            &mut cursor,
                        )
                        .expect("derap stringtable")
                        .to_writer(&mut writer, true)
                        {
                            error!("failed to write project to stringtable: {}", e);
                            todo!("handle error properly");
                        }
                        writer.as_bytes().to_vec()
                    }
                    _ => hemtt_config::Config::derapify(&mut cursor)?
                        .to_string()
                        .as_bytes()
                        .to_vec(),
                };
                raw_out.write_all(&derap)?;
                cursor.set_position(0);
            }
            out.write_all(&cursor.into_inner())?;
            continue;
        }

        std::io::copy(&mut file, &mut out)?;
    }
    Ok(())
}

/// Accept only portable, ordinary relative file names. Check Windows aliases
/// on every host so an archive cannot acquire a different meaning on Windows.
fn entry_path(filename: &str) -> io::Result<PathBuf> {
    let mut path = PathBuf::new();
    for part in filename.split(['/', '\\']) {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end_matches(' ')
            .to_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            || device
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsafe PBO entry path: {filename:?}"),
            ));
        }
        path.push(part);
    }
    Ok(path)
}

fn is_directory_link(metadata: &Metadata) -> bool {
    let link = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let link = {
        use std::os::windows::fs::MetadataExt as _;
        // Include junctions and other reparse points, not just symbolic links.
        link || metadata.file_attributes() & 0x400 != 0
    };
    link
}

fn check_output_directory(root: &Path, directory: &Path) -> io::Result<()> {
    let metadata = fs_err::symlink_metadata(directory)?;
    if !metadata.is_dir()
        || is_directory_link(&metadata)
        || !fs_err::canonicalize(directory)?.starts_with(root)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unsafe PBO output directory: {}", directory.display()),
        ));
    }
    Ok(())
}

/// PBOs contain regular files only. Never reuse links or overwrite another
/// entry, generated metadata, a derapified file, or an existing hard link.
fn create_output_file(root: &Path, relative: &Path) -> io::Result<File> {
    check_output_directory(root, root)?;
    let mut directory = root.to_path_buf();
    for component in relative.parent().into_iter().flat_map(Path::components) {
        directory.push(component);
        match fs_err::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        check_output_directory(root, &directory)?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(relative))
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor, path::Path};

    use hemtt_pbo::WritablePbo;
    use hemtt_test::directory::TemporaryDirectory;

    use super::{PboUnpackArgs, create_output_file, entry_path, execute};

    fn archive(root: &Path, files: &[(&str, &[u8])]) -> PboUnpackArgs {
        let mut pbo = WritablePbo::new();
        pbo.add_property("prefix", "test\\addon");
        pbo.add_property("version", "1");
        pbo.add_property("author", "test");
        for (name, data) in files {
            pbo.add_file(*name, Cursor::new(*data)).expect("add file");
        }
        let pbo_path = root.join("test.pbo");
        pbo.write(&mut fs::File::create(&pbo_path).expect("PBO file"), true)
            .expect("write PBO");
        PboUnpackArgs {
            pbo: pbo_path.to_string_lossy().into_owned(),
            output: Some(root.join("unpacked").to_string_lossy().into_owned()),
            derap: false,
        }
    }

    #[test]
    fn rejects_unsafe_entry_paths_on_every_host() {
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "..\\escape",
            "nested/../../escape",
            "nested\\..\\escape",
            "/absolute",
            "\\absolute",
            "//server/share/file",
            "\\\\server\\share\\file",
            "C:/absolute",
            "C:\\absolute",
            "C:relative",
            "\\\\?\\C:\\file",
            "\\\\.\\NUL",
            "file:stream",
            "nested/file:stream",
            "nested//file",
            "nested/./file",
            "nested/",
            "nested\\",
            ".. /file",
            "name./file",
            "name /file",
            "NUL",
            "con.txt",
            "nested/AUX.sqf",
            "COM1",
            "lpt9.txt",
            "com¹.txt",
            "LPT²",
            "PRN",
            "con .txt",
            "COM1 .sqf",
            "CONIN$",
            "CONOUT$",
            "bad\0file",
            "bad\nfile",
            "bad<file",
            "bad>file",
            "bad\"file",
            "bad|file",
            "bad?file",
            "bad*file",
        ] {
            assert!(entry_path(name).is_err(), "accepted {name:?}");
        }
    }

    #[test]
    fn accepts_nested_paths_and_both_separators() {
        for name in [
            "file.sqf",
            "nested/file.sqf",
            "nested\\file.sqf",
            "a\\b/file.sqf",
            "日本語/ä.sqf",
            ".hidden",
            "COM10.txt",
        ] {
            assert!(entry_path(name).is_ok(), "rejected {name:?}");
        }
        assert_eq!(
            entry_path("a\\b/file.sqf").expect("path"),
            Path::new("a").join("b").join("file.sqf")
        );
    }

    #[test]
    fn rejects_traversal_before_writing_any_output() {
        let root = TemporaryDirectory::new();
        let sentinel = root.join("escape.txt");
        fs::write(&sentinel, b"keep me").expect("sentinel");
        let args = archive(
            &root,
            &[("a.txt", b"normal"), ("z/../../escape.txt", b"overwrite")],
        );
        let error = execute(&args).expect_err("reject traversal");
        assert!(error.to_string().contains("Unsafe PBO entry path"));
        assert!(!root.join("unpacked").exists());
        assert_eq!(fs::read(sentinel).expect("sentinel"), b"keep me");
    }

    #[test]
    fn rejects_absolute_drive_unc_and_stream_entries_before_output() {
        let root = TemporaryDirectory::new();
        for name in [
            "/absolute.txt",
            "C:\\absolute.txt",
            "C:relative.txt",
            "\\\\server\\share\\file.txt",
            "file.txt:stream",
        ] {
            let args = archive(&root, &[(name, b"untrusted")]);
            assert!(execute(&args).is_err(), "accepted {name:?}");
            assert!(
                !root.join("unpacked").exists(),
                "created output for {name:?}"
            );
        }
    }

    #[test]
    fn unpacks_regular_files_and_properties() {
        let root = TemporaryDirectory::new();
        let args = archive(
            &root,
            &[("nested/a.sqf", b"one"), ("nested\\b.sqf", b"two")],
        );
        execute(&args).expect("unpack");
        assert_eq!(
            fs::read(root.join("unpacked/nested/a.sqf")).expect("a"),
            b"one"
        );
        assert_eq!(
            fs::read(root.join("unpacked/nested/b.sqf")).expect("b"),
            b"two"
        );
        assert_eq!(
            fs::read(root.join("unpacked/$PBOPREFIX$")).expect("prefix"),
            b"test\\addon"
        );
        assert_eq!(
            fs::read_to_string(root.join("unpacked/properties.txt")).expect("properties"),
            "version=1\nauthor=test\n"
        );
        assert!(
            execute(&args).is_err(),
            "must not reuse an existing output directory"
        );
    }

    #[test]
    fn archive_cannot_overwrite_generated_metadata() {
        let root = TemporaryDirectory::new();
        let args = archive(&root, &[("$PBOPREFIX$", b"overwrite")]);
        assert!(execute(&args).is_err());
        assert_eq!(
            fs::read(root.join("unpacked/$PBOPREFIX$")).expect("prefix"),
            b"test\\addon"
        );
    }

    #[test]
    fn derapified_files_remain_inside_output_and_cannot_be_overwritten() {
        let root = TemporaryDirectory::new();
        let binary =
            include_bytes!("../../../../libs/config/tests/rapify/single_class/expected.bin");
        let mut args = archive(&root, &[("nested/config.bin", binary)]);
        args.derap = true;
        execute(&args).expect("derap unpack");
        assert_eq!(
            fs::read(root.join("unpacked/nested/config.bin")).expect("binary"),
            binary
        );
        let text = fs::read(root.join("unpacked/nested/config.cpp")).expect("derapified config");
        assert!(!text.is_empty());

        // A later archive entry must not replace the text generated by --derap.
        args = archive(
            &root,
            &[("config.bin", binary), ("config.cpp", b"overwrite")],
        );
        args.output = Some(root.join("collision").to_string_lossy().into_owned());
        args.derap = true;
        assert!(execute(&args).is_err());
        assert_eq!(
            fs::read(root.join("collision/config.cpp")).expect("preserved text"),
            text
        );
    }

    #[test]
    fn existing_files_and_hard_links_are_never_overwritten() {
        let root = TemporaryDirectory::new();
        let output = root.join("unpacked");
        fs::create_dir(&output).expect("output");
        let output = fs::canonicalize(output).expect("canonical output");
        let sentinel = root.join("sentinel.txt");
        fs::write(&sentinel, b"keep me").expect("sentinel");
        fs::hard_link(&sentinel, output.join("file.txt")).expect("hard link");
        assert!(create_output_file(&output, Path::new("file.txt")).is_err());
        assert_eq!(fs::read(sentinel).expect("sentinel"), b"keep me");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn directory_links_cannot_redirect_output() {
        let root = TemporaryDirectory::new();
        let output = root.join("unpacked");
        let outside = root.join("outside");
        fs::create_dir(&output).expect("output");
        fs::create_dir(&outside).expect("outside");
        let output = fs::canonicalize(output).expect("canonical output");
        let link = output.join("nested");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).expect("directory symlink");
        #[cfg(windows)]
        assert!(
            std::process::Command::new("cmd")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&link)
                .arg(&outside)
                .output()
                .expect("create junction")
                .status
                .success()
        );
        let result = create_output_file(&output, Path::new("nested/escape.txt"));
        let mut args = archive(&root, &[("escape.txt", b"untrusted")]);
        args.output = Some(link.to_string_lossy().into_owned());
        let existing_output_result = execute(&args);
        #[cfg(windows)]
        fs::remove_dir(&link).expect("remove junction");
        #[cfg(unix)]
        fs::remove_file(&link).expect("remove directory symlink");
        assert!(result.is_err(), "must reject a directory link");
        assert!(
            existing_output_result.is_err(),
            "must reject a linked output root"
        );
        assert!(!outside.join("escape.txt").exists());
    }
}
