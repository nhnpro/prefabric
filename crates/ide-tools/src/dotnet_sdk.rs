//! A private .NET SDK for the C# tools when the machine has none new enough.
//! Roslyn's language server and csharp-ls run on .NET 10 and load projects with
//! MSBuild, which only an SDK (not a runtime) carries. A system SDK of that
//! major version or newer is used as is; otherwise the latest SDK of the
//! channel is downloaded from Microsoft, checked against its published SHA-512
//! and unpacked into `<tools>/dotnet-sdk`.

use crate::process::{Emit, Http, run_quiet};
use crate::{InstallEvent, Tools, fetch_verified, sources};
use ide_core::error::ErrorKind;
use ide_core::{CancellationToken, IdeError, IdeResult};
use std::path::PathBuf;
use std::time::Duration;

pub(crate) const CHANNEL: &str = "10.0";
const MAJOR: u32 = 10;
pub(crate) const DIR: &str = "dotnet-sdk";

/// The major versions in `dotnet --list-sdks` output (`10.0.101 [/usr/local/share/dotnet/sdk]`).
pub(crate) fn sdk_majors(list: &str) -> Vec<u32> {
    list.lines().filter_map(|l| l.split('.').next()?.trim().parse().ok()).collect()
}

impl Tools {
    fn private_dotnet(&self) -> Option<PathBuf> {
        self.exe_in(&self.root.join(DIR), "dotnet")
    }

    fn has_sdk(&self, dotnet: &std::path::Path) -> bool {
        let mut c = self.dotnet_command(dotnet);
        c.arg("--list-sdks");
        run_quiet(c, Duration::from_secs(20))
            .is_some_and(|(ok, out)| ok && sdk_majors(&out).into_iter().any(|m| m >= MAJOR))
    }

    /// The `dotnet` the C# tools are installed and run with: the system one when
    /// it has an SDK of at least [`MAJOR`], else the private one when installed.
    pub(crate) fn dotnet(&self) -> Option<PathBuf> {
        self.which("dotnet").filter(|d| self.has_sdk(d)).or_else(|| self.private_dotnet())
    }

    /// [`Tools::dotnet`], installing the private SDK first when there is none.
    pub(crate) fn ensure_dotnet(&self, cancel: &CancellationToken, emit: Emit) -> IdeResult<PathBuf> {
        if let Some(dotnet) = self.dotnet() {
            return Ok(dotnet);
        }
        emit(InstallEvent::Line {
            text: format!("No .NET {MAJOR} SDK found: installing a private copy for the IDE's C# tools"),
        });
        let http = Http::new(&self.system_path)?;
        let json = http.get_text(&sources::dotnet_releases_url(CHANNEL), cancel)?;
        let dl = sources::dotnet_sdk_pick(&json, self.platform)?;
        emit(InstallEvent::Line { text: format!("Downloading .NET SDK {} ({})", dl.version, dl.url) });
        let staging = self.root.join(".staging-dotnet-sdk");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;
        let result = (|| {
            let archive = fetch_verified(&self.downloads, &http, &dl, &staging, cancel, emit)?;
            emit(InstallEvent::Line { text: "Extracting…".into() });
            let pkg = staging.join("pkg");
            crate::archive::extract(
                &archive,
                crate::archive::ArchiveKind::from_name(&dl.name),
                &pkg,
                "dotnet",
                cancel,
            )?;
            if self.exe_in(&pkg, "dotnet").is_none() {
                return Err(IdeError::new(ErrorKind::Io, format!("`dotnet` was not found in {}", dl.name)));
            }
            let dir = self.root.join(DIR);
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::rename(&pkg, &dir)?;
            crate::clear_quarantine(&dir);
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result?;
        let dotnet = self
            .private_dotnet()
            .ok_or_else(|| IdeError::new(ErrorKind::Io, "the .NET SDK did not install `dotnet`"))?;
        crate::archive::make_executable(&dotnet)?;
        emit(InstallEvent::Line { text: format!(".NET SDK {} installed", dl.version) });
        Ok(dotnet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sdk_majors() {
        let list = "8.0.416 [/usr/local/share/dotnet/sdk]\n10.0.101 [/usr/local/share/dotnet/sdk]\n";
        assert_eq!(sdk_majors(list), [8, 10]);
        assert!(sdk_majors("").is_empty());
        assert!(sdk_majors("garbage\n").is_empty());
    }
}
