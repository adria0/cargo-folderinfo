use std::fs;
use std::path::Path;

extern crate itertools;
extern crate toml;

use std::string::ToString;
use toml::Value;

#[derive(Debug)]
struct Project {
    folder: String,
    name: Option<String>,
    desc: Option<String>,
    sub: Vec<Project>,
    dep: Vec<String>,
}

fn v2table(v: &toml::Value) -> &toml::value::Table {
    match v {
        Value::Table(t) => t,
        _ => panic!(format!("expected {:?} to be a table", v)),
    }
}
fn v2str(v: &toml::Value) -> &String {
    match v {
        Value::String(s) => s,
        _ => panic!(format!("expected {:?} to be a string", v)),
    }
}

fn process_folder(path: &Path) -> Option<Project> {
    if let Ok(toml_content) = fs::read_to_string(path.join("Cargo.toml")) {
        let toml = toml_content.parse::<Value>().expect("Unable to parse toml");

        let mut sub = Vec::new();
        let mut dep = Vec::new();
        let mut name = None;
        let mut desc = None;
        for (k, v) in v2table(&toml) {
            match k.as_str() {
                "package" => {
                    desc = v2table(v).get("description").map(|v| v2str(v)).cloned();
                    name = v2table(v).get("name").map(|v| v2str(v)).cloned();
                }
                "dependencies" => {
                    let depends = match v {
                        Value::Table(depends) => depends,
                        _ => panic!("bad dependencies"),
                    };

                    dep.extend_from_slice(&depends.keys().cloned().collect::<Vec<_>>());
                }
                _ => {
                    if k.starts_with("dependencies.") {
                        let (_, dependency) = k.split_at("dependencies.".len());
                        dep.push(dependency.to_string());
                    }
                }
            }
        }

        for entry in path.read_dir().unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                if let Some(sub_project) = process_folder(&entry) {
                    sub.push(sub_project);
                }
            }
        }
        let folder = path
            .file_name()
            .map_or("".to_string(), |v| v.to_os_string().into_string().unwrap());
        Some(Project {
            desc,
            sub,
            dep,
            name,
            folder,
        })
    } else {
        None
    }
}

fn pad_text(s: &str, max_column_size: usize) -> Vec<String> {
    let mut current = "".to_string();
    let mut formatted = Vec::new();
    for word in s.split(' ') {
        if word.len() + current.len() + 1 > max_column_size {
            formatted.push(current);
            current = word.to_owned();
            current.push(' ');
        } else {
            current.push_str(word);
            current.push(' ');
        }
    }
    formatted.push(current);
    formatted
}

fn collect_crate_names(p: &Project) -> Vec<String> {
    fn collect(p: &Project, names: &mut Vec<String>) {
        names.push(p.name.clone().unwrap());
        for sub in &p.sub {
            collect(sub, names);
        }
    }
    let mut names = Vec::new();
    collect(p, &mut names);
    names
}

fn print_project(p: &Project) {
    fn repeat(s: &str, n: usize) -> String {
        (0..n).map(|_| s).collect::<String>()
    }

    fn print_project_1(p: &Project, crates: &[String], level: usize) {
        let max_size = 80;

        let mut text = String::from("[");
        text.push_str(p.name.as_ref().map_or("", |v| v));
        text.push_str("] ");
        text.push_str(p.desc.as_ref().map_or("<no desc>", |d| &d));

        let left_margin = repeat("   |", level);
        let folder_spaces = repeat(" ", p.folder.len());
        let padded_text_len = max_size - p.folder.len() + 3 + 4 * level;

        for (n, line) in pad_text(&text, padded_text_len).iter().enumerate() {
            if n == 0 {
                println!("{}{} - {}", left_margin, p.folder, line);
            } else {
                println!("{}{}   {}", left_margin, folder_spaces, line);
            }
        }

        let (in_workspace, outside_workspace): (Vec<_>, Vec<_>) = p
            .dep
            .iter()
            .partition(|dep| crates.iter().any(|p| &p == dep));
        let (in_workspace, outside_workspace) = (
            itertools::join(in_workspace, ", "),
            itertools::join(outside_workspace, ", "),
        );

        if !in_workspace.is_empty() {
            for line in pad_text(&in_workspace, padded_text_len).iter() {
                println!("{}{}   < {}", left_margin, folder_spaces, line);
            }
        }
        if !outside_workspace.is_empty() {
            for line in pad_text(&outside_workspace, padded_text_len).iter() {
                println!("{}{}   > {}", left_margin, folder_spaces, line);
            }
        }
        for sub in p.sub.iter() {
            println!("{}", repeat("   |", level));
            print_project_1(&sub, &crates, level + 1);
        }
    }
    let crates = collect_crate_names(p);
    print_project_1(p, &crates, 0);
}

fn main() {
    let all = process_folder(Path::new("."));
    println!("Rust folderinfo dump format:");
    println!(" <folder-name> - [<crate-name>] crate-description");
    println!("                 < internal dependencies");
    println!("                 > external dependencies");
    println!();

    print_project(&all.unwrap());
}
