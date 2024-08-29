use std::path::PathBuf;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    pub fn create(basename: impl Into<String>) -> Self {
        let mut rng = thread_rng();
        let basename: String = basename.into();

        for _ in 0..16 {
            let mut name = basename.clone();
            name.push('_');
            name.extend((&mut rng).sample_iter(Alphanumeric).take(8).map(char::from));

            let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
            if std::fs::create_dir(&path).is_ok() {
                eprintln!("Created temp dir {path:?}");
                return Self { path };
            }
        }
        panic!("Could not create temp dir with basename {basename}")
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub fn delete(self) {
        let _ = std::fs::remove_dir_all(self.path);
    }
}
