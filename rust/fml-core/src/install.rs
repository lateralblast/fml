//! How to install the commands fml needs (`mkvtoolnix`, `mediainfo`, `ffmpeg`) on each system.

use std::fs;

use crate::util::which;

/// The package manager `--install` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer {
    /// Debian, Ubuntu and what they are built on.
    Apt,
    /// Fedora, RHEL and what they are built on.
    Dnf,
    /// Arch Linux and what it is built on.
    Pacman,
    /// macOS, with Homebrew.
    Brew,
    /// Windows, with winget.
    Winget,
    /// Windows, with Chocolatey.
    Choco,
}

impl Installer {
    /// The command that installs one of the packages fml needs, named as in `REQUIRED_TOOLS`
    /// (`mkvtoolnix`, `mediainfo`, `ffmpeg`), under this package manager's own name for it.
    pub fn command(self, package: &str) -> Vec<String> {
        let words: Vec<&str> = match self {
            Installer::Apt => vec!["sudo", "apt", "install", "-y", package],
            Installer::Dnf => {
                // Fedora's own ffmpeg is the free build; the full one needs RPM Fusion
                let name = if package == "ffmpeg" { "ffmpeg-free" } else { package };
                vec!["sudo", "dnf", "install", "-y", name]
            }
            Installer::Pacman => {
                let name = if package == "mkvtoolnix" { "mkvtoolnix-cli" } else { package };
                vec!["sudo", "pacman", "-S", "--noconfirm", "--needed", name]
            }
            Installer::Brew => vec!["brew", "install", package],
            Installer::Winget => {
                let id = match package {
                    "mkvtoolnix" => "MoritzBunkus.MKVToolNix",
                    "mediainfo" => "MediaArea.MediaInfo",
                    "ffmpeg" => "Gyan.FFmpeg",
                    other => other,
                };
                vec![
                    "winget", "install", "--id", id, "--exact", "--accept-source-agreements",
                    "--accept-package-agreements",
                ]
            }
            Installer::Choco => {
                let name = if package == "mediainfo" { "mediainfo-cli" } else { package };
                vec!["choco", "install", "-y", name]
            }
        };
        words.iter().map(|w| w.to_string()).collect()
    }

    /// What it is called in messages.
    pub fn name(self) -> &'static str {
        match self {
            Installer::Apt => "apt",
            Installer::Dnf => "dnf",
            Installer::Pacman => "pacman",
            Installer::Brew => "brew",
            Installer::Winget => "winget",
            Installer::Choco => "choco",
        }
    }
}

/// The package manager of a Linux distribution from the `ID` and `ID_LIKE` of its
/// `/etc/os-release` (a distribution that is built on another says so in `ID_LIKE`).
pub fn linux_installer(id: &str, id_like: &str) -> Option<Installer> {
    let family = |word: &str| match word {
        "debian" | "ubuntu" | "linuxmint" | "pop" | "raspbian" | "elementary" | "zorin" | "kali" => {
            Some(Installer::Apt)
        }
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "ol" => Some(Installer::Dnf),
        "arch" | "manjaro" | "endeavouros" | "garuda" | "artix" => Some(Installer::Pacman),
        _ => None,
    };
    // its own name first, then the ones it is built on, in the order given
    std::iter::once(id).chain(id_like.split_whitespace()).find_map(|word| family(&word.to_lowercase()))
}

/// The `ID` and `ID_LIKE` of this Linux system, lower case and unquoted (empty if not known).
pub fn os_release() -> (String, String) {
    let text = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_lowercase())
            .unwrap_or_default()
    };
    (value("ID="), value("ID_LIKE="))
}

/// The package manager of this system, if fml knows one: Homebrew on macOS, the distribution's on
/// Linux, and on Windows winget or else Chocolatey, whichever is installed.
pub fn detect_installer() -> Option<Installer> {
    match std::env::consts::OS {
        "macos" => Some(Installer::Brew),
        "linux" => {
            let (id, like) = os_release();
            linux_installer(&id, &like)
        }
        "windows" => {
            if which("winget").is_some() {
                Some(Installer::Winget)
            } else if which("choco").is_some() {
                Some(Installer::Choco)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_distribution_is_known_by_its_own_id_or_the_one_it_is_built_on() {
        assert_eq!(linux_installer("ubuntu", "debian"), Some(Installer::Apt));
        assert_eq!(linux_installer("debian", ""), Some(Installer::Apt));
        assert_eq!(linux_installer("linuxmint", "ubuntu debian"), Some(Installer::Apt));
        assert_eq!(linux_installer("fedora", ""), Some(Installer::Dnf));
        assert_eq!(linux_installer("rocky", "rhel centos fedora"), Some(Installer::Dnf));
        assert_eq!(linux_installer("arch", ""), Some(Installer::Pacman));
        assert_eq!(linux_installer("endeavouros", "arch"), Some(Installer::Pacman));
        assert_eq!(linux_installer("somethingnew", "ubuntu"), Some(Installer::Apt)); // by what it is built on
        assert_eq!(linux_installer("alpine", ""), None);
        assert_eq!(linux_installer("", ""), None);
        assert_eq!(linux_installer("Ubuntu", ""), Some(Installer::Apt)); // case does not matter
    }

    fn words(installer: Installer, package: &str) -> String {
        installer.command(package).join(" ")
    }

    #[test]
    fn each_package_manager_gets_its_own_package_names() {
        assert_eq!(words(Installer::Apt, "mkvtoolnix"), "sudo apt install -y mkvtoolnix");
        assert_eq!(words(Installer::Brew, "ffmpeg"), "brew install ffmpeg");
        assert_eq!(words(Installer::Dnf, "ffmpeg"), "sudo dnf install -y ffmpeg-free");
        assert_eq!(words(Installer::Dnf, "mediainfo"), "sudo dnf install -y mediainfo");
        assert_eq!(words(Installer::Pacman, "mkvtoolnix"), "sudo pacman -S --noconfirm --needed mkvtoolnix-cli");
        assert_eq!(words(Installer::Pacman, "ffmpeg"), "sudo pacman -S --noconfirm --needed ffmpeg");
        assert_eq!(
            words(Installer::Winget, "mediainfo"),
            "winget install --id MediaArea.MediaInfo --exact --accept-source-agreements --accept-package-agreements"
        );
        assert!(words(Installer::Winget, "mkvtoolnix").contains("--id MoritzBunkus.MKVToolNix "));
        assert!(words(Installer::Winget, "ffmpeg").contains("--id Gyan.FFmpeg "));
        assert_eq!(words(Installer::Choco, "mediainfo"), "choco install -y mediainfo-cli");
        assert_eq!(words(Installer::Choco, "mkvtoolnix"), "choco install -y mkvtoolnix");
    }

    #[test]
    fn every_package_fml_needs_has_a_command_for_every_installer() {
        for installer in [Installer::Apt, Installer::Dnf, Installer::Pacman, Installer::Brew, Installer::Winget, Installer::Choco] {
            for (_, package) in crate::model::REQUIRED_TOOLS {
                let command = installer.command(package);
                assert!(command.len() >= 3 && !command.iter().any(String::is_empty), "{installer:?} {package}");
            }
        }
    }
}
