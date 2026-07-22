use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    pub state_dir: PathBuf,
    pub mobile_passcode_file: PathBuf,
    pub trusted_browsers_file: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Self {
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."));
        let state_home = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".local/state")))
            .unwrap_or_else(|| PathBuf::from("."));

        let config_dir = config_home.join("ai-scheduler");
        let data_dir = data_home.join("ai-scheduler");
        let state_dir = state_home.join("ai-scheduler");

        Self {
            config_file: config_dir.join("config.toml"),
            db_file: data_dir.join("runs.db"),
            data_dir,
            mobile_passcode_file: config_dir.join("mobile-passcode"),
            trusted_browsers_file: state_dir.join("mobile-trusted-browsers"),
            state_dir,
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_xdg_paths_when_present() {
        let temp = tempfile::tempdir().unwrap();
        env::set_var("XDG_CONFIG_HOME", temp.path().join("config"));
        env::set_var("XDG_DATA_HOME", temp.path().join("data"));
        env::set_var("XDG_STATE_HOME", temp.path().join("state"));

        let paths = AppPaths::discover();

        assert_eq!(
            paths.config_file,
            temp.path().join("config/ai-scheduler/config.toml")
        );
        assert_eq!(paths.db_file, temp.path().join("data/ai-scheduler/runs.db"));
        assert_eq!(paths.state_dir, temp.path().join("state/ai-scheduler"));
        assert_eq!(
            paths.mobile_passcode_file,
            temp.path().join("config/ai-scheduler/mobile-passcode")
        );
        assert_eq!(
            paths.trusted_browsers_file,
            temp.path()
                .join("state/ai-scheduler/mobile-trusted-browsers")
        );

        env::remove_var("XDG_CONFIG_HOME");
        env::remove_var("XDG_DATA_HOME");
        env::remove_var("XDG_STATE_HOME");
    }
}
