use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("usage: oxide new <path> --name <package> [--force]")]
    Usage,
    #[error("target directory '{0}' is not empty; pass --force to overwrite scaffold files")]
    NonEmptyDirectory(String),
    #[error("invalid package name '{0}'")]
    InvalidPackageName(String),
    #[error("io error at '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NewProjectOptions {
    path: PathBuf,
    name: String,
    force: bool,
}

fn main() {
    if let Err(err) = run(env::args().skip(1).collect()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), CliError> {
    let options = parse_new_args(args)?;
    scaffold_project(&options)
}

fn parse_new_args(args: Vec<String>) -> Result<NewProjectOptions, CliError> {
    if args.len() < 4 || args.first().map(String::as_str) != Some("new") {
        return Err(CliError::Usage);
    }

    let path = PathBuf::from(&args[1]);
    let mut name = None;
    let mut force = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--name" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Usage);
                };
                name = Some(value.clone());
            }
            "--force" => force = true,
            _ => return Err(CliError::Usage),
        }
        index += 1;
    }

    let name = name.ok_or(CliError::Usage)?;
    if !is_valid_package_name(&name) {
        return Err(CliError::InvalidPackageName(name));
    }

    Ok(NewProjectOptions { path, name, force })
}

fn scaffold_project(options: &NewProjectOptions) -> Result<(), CliError> {
    if options.path.exists() && !options.force && is_non_empty_dir(&options.path)? {
        return Err(CliError::NonEmptyDirectory(
            options.path.display().to_string(),
        ));
    }

    create_dir_all(&options.path)?;
    create_dir_all(&options.path.join("src"))?;
    create_dir_all(&options.path.join("assets/scenes"))?;

    write_file(
        &options.path.join("Cargo.toml"),
        &cargo_toml(&options.name, engine_dependency()),
    )?;
    write_file(&options.path.join("src/main.rs"), MAIN_RS)?;
    write_file(
        &options.path.join("assets/scenes/starter.oxscene"),
        STARTER_OXSCENE,
    )?;

    Ok(())
}

fn is_non_empty_dir(path: &Path) -> Result<bool, CliError> {
    if !path.is_dir() {
        return Ok(false);
    }

    let mut entries = fs::read_dir(path).map_err(|source| CliError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(entries.next().is_some())
}

fn create_dir_all(path: &Path) -> Result<(), CliError> {
    fs::create_dir_all(path).map_err(|source| CliError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn write_file(path: &Path, contents: &str) -> Result<(), CliError> {
    fs::write(path, contents).map_err(|source| CliError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn cargo_toml(name: &str, engine_dependency: EngineDependency) -> String {
    let engine_dependency = match engine_dependency {
        EngineDependency::Published => {
            r#"oxide_engine = { package = "oxide-core-engine", version = "0.1" }"#.to_string()
        }
        EngineDependency::Path(path) => format!(
            r#"oxide_engine = {{ package = "oxide-core-engine", path = "{}" }}"#,
            path.display()
        ),
    };

    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
{engine_dependency}
tracing-subscriber = "0.3"
"#
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EngineDependency {
    Published,
    Path(PathBuf),
}

fn engine_dependency() -> EngineDependency {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine_path = manifest_dir
        .parent()
        .map(|crates_dir| crates_dir.join("oxide_engine"));
    match engine_path {
        Some(path) if path.join("Cargo.toml").exists() => EngineDependency::Path(path),
        _ => EngineDependency::Published,
    }
}

fn is_valid_package_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

const MAIN_RS: &str = r#"use oxide_engine::prelude::*;

struct Game {
    world: World,
    scene_handle: Handle<SceneDescriptor>,
    scene_loaded: bool,
}

impl App for Game {
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
    }

    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);
        world.insert_resource(RendererResource::new(renderer));
        world.insert_resource(WindowResource::new(window.size().width, window.size().height));

        let scene_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/scenes/starter.oxscene");
        let scene_handle = request_oxscene_spawn(&mut world, scene_path);

        Self {
            world,
            scene_handle,
            scene_loaded: false,
        }
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    fn update(&mut self) {
        if !self.scene_loaded {
            if let Some(roots) = take_spawned_oxscene_roots(&mut self.world, self.scene_handle) {
                self.world.insert_resource(SceneSpawnResult { entities: roots });
                self.scene_loaded = true;
            }
        }
    }

    fn on_event(&mut self, _event: EngineEvent) {}
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<Game>()
        .add_plugins(DefaultPlugins)
        .add_plugins(SceneAuthoringPlugins)
        .add_system(AppStage::PreUpdate, camera_controller_system)
        .run();
}
"#;

const STARTER_OXSCENE: &str = r#"{
  "format": "oxide.oxscene",
  "version": 1,
  "scene": {
    "entities": [
      {
        "name": "Camera",
        "transform": { "position": [0.0, 2.0, 6.0] },
        "type": "camera",
        "target": [0.0, 0.0, 0.0],
        "controller": true
      },
      {
        "name": "Key Light",
        "type": "directional_light",
        "direction": [0.8, -1.0, -0.4],
        "color": [1.0, 0.96, 0.88],
        "intensity": 0.8
      },
      {
        "name": "Ambient",
        "type": "ambient_light",
        "color": [0.45, 0.48, 0.55],
        "intensity": 0.25
      },
      {
        "name": "Cube",
        "type": "mesh",
        "primitive": "cube",
        "material": {
          "name": "default_unlit",
          "shader": "unlit",
          "color": [0.85, 0.72, 0.48, 1.0]
        }
      }
    ]
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_new_project_args() {
        let options = parse_new_args(vec![
            "new".to_string(),
            "my_game".to_string(),
            "--name".to_string(),
            "my_game".to_string(),
            "--force".to_string(),
        ])
        .unwrap();

        assert_eq!(options.path, PathBuf::from("my_game"));
        assert_eq!(options.name, "my_game");
        assert!(options.force);
    }

    #[test]
    fn scaffold_writes_expected_files() {
        let path = temp_path("oxide_scaffold");
        let options = NewProjectOptions {
            path: path.clone(),
            name: "scaffold_game".to_string(),
            force: false,
        };

        scaffold_project(&options).unwrap();
        assert!(path.join("Cargo.toml").exists());
        assert!(path.join("src/main.rs").exists());
        assert!(path.join("assets/scenes/starter.oxscene").exists());

        let cargo = fs::read_to_string(path.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"scaffold_game\""));
        assert!(cargo.contains("oxide_engine = { package = \"oxide-core-engine\", path = "));

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn cargo_toml_can_use_published_engine_dependency() {
        let cargo = cargo_toml("published_game", EngineDependency::Published);
        assert!(
            cargo.contains("oxide_engine = { package = \"oxide-core-engine\", version = \"0.1\" }")
        );
    }

    #[test]
    fn scaffold_refuses_non_empty_directory_without_force() {
        let path = temp_path("oxide_scaffold_non_empty");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("existing.txt"), "keep").unwrap();

        let options = NewProjectOptions {
            path: path.clone(),
            name: "scaffold_game".to_string(),
            force: false,
        };

        let err = scaffold_project(&options).unwrap_err();
        assert!(matches!(err, CliError::NonEmptyDirectory(_)));
        let _ = fs::remove_dir_all(path);
    }

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{name}_{stamp}"))
    }
}
