use std::fs;
use std::path::Path;

extern crate itertools;
extern crate toml;

use std::collections::HashMap;
use std::string::ToString;
use toml::Value;

use structopt::clap::arg_enum;
use structopt::StructOpt;

arg_enum! {
    #[derive(Debug)]
    enum OutputFormat {
        Dot,
        Text,
    }
}

struct Void {}
type StringSet = HashMap<String, Void>;

#[derive(Debug, StructOpt)]
#[structopt(name = "folderinfo", about = "Crate info")]
struct Opt {
    /// Select output format, can be Text (default) or Dot
    #[structopt(long)]
    format: Option<OutputFormat>,
    /// Comma-separated list of crates to ignore
    #[structopt(long)]
    ignore: Option<String>,
    /// Comma-separated list of crates to highlight: all_directions,+from,-to
    #[structopt(long)]
    highlight: Option<String>,
}

#[derive(Debug)]
struct Project {
    folder: String,
    name: String,
    desc: Option<String>,
    subs: Vec<Project>,
    deps: Vec<String>,
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

        let mut subs = Vec::new();
        let mut deps = Vec::new();
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

                    deps.extend_from_slice(&depends.keys().cloned().collect::<Vec<_>>());
                }
                _ => {
                    if k.starts_with("dependencies.") {
                        let (_, dependency) = k.split_at("dependencies.".len());
                        deps.push(dependency.to_string());
                    }
                }
            }
        }

        for entry in path.read_dir().unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                if let Some(sub_project) = process_folder(&entry) {
                    subs.push(sub_project);
                }
            }
        }
        let folder = path
            .file_name()
            .map_or("".to_string(), |v| v.to_os_string().into_string().unwrap());

        Some(Project {
            desc,
            subs,
            deps,
            name: name.unwrap_or_else(|| panic!(format!("Crate without name in {:?}", path))),
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

fn collect_crate_names(p: &Project) -> StringSet {
    fn collect(p: &Project, names: &mut StringSet) {
        names.insert(p.name.clone(), Void {});
        for sub in &p.subs {
            collect(sub, names);
        }
    }
    let mut names = StringSet::new();
    collect(p, &mut names);
    names
}

fn print_project_text(p: &Project) {
    fn repeat(s: &str, n: usize) -> String {
        (0..n).map(|_| s).collect::<String>()
    }

    fn print_project_1(p: &Project, crates: &StringSet, level: usize) {
        let max_size = 80;

        let mut text = format!("[{}] ", p.name);
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

        let (in_workspace, outside_workspace): (Vec<_>, Vec<_>) =
            p.deps.iter().partition(|dep| crates.contains_key(*dep));

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
        for sub in p.subs.iter() {
            println!("{}", repeat("   |", level));
            print_project_1(&sub, &crates, level + 1);
        }
    }
    let crates = collect_crate_names(p);

    println!("dump format:");
    println!(" <folder-name> - [<crate-name>] crate-description");
    println!("                 < internal dependencies");
    println!("                 > external dependencies");
    println!();
    print_project_1(p, &crates, 0);
}

fn print_project_dot(p: &Project, ignore_crates: &StringSet, highlight_crates: &StringSet) {
    fn print_clusters_1(p: &Project) {
        if !p.subs.is_empty() {
            println!("subgraph cluster_{} {{", p.name.replace("-", "_"));
            println!("label=\"{}\"", p.name);
        }
        println!("\"{}\"", p.name);
        for sub in p.subs.iter() {
            print_clusters_1(&sub);
        }
        if !p.subs.is_empty() {
            println!("}}");
        }
    }
    fn print_nodes_1(p: &Project, ignore: &StringSet, highlight: &StringSet, crates: &StringSet) {
        let internal_deps = p.deps.iter().filter(|dep| crates.keys().any(|p| &p == dep));
        for dep in internal_deps {
            let (from, to) = (&p.name, dep);

            let no_ignores = || !ignore.contains_key(from) && !ignore.contains_key(to);
            let internal_dep_is_a_submodule = || p.subs.iter().any(|sub| &sub.name == dep);

            if no_ignores() && !internal_dep_is_a_submodule() {
                let attrs = if highlight.is_empty() {
                    ""
                } else {
                    let mut color = "[color=transparent]";
                    for h in highlight.keys() {
                        if h.starts_with('+') && from == &h[1..] {
                            color = "[color=blue]";
                            break;
                        } else if h.starts_with('-') && to == &h[1..] {
                            color = "[color=red]";
                            break;
                        } else if from == h || to == h {
                            color = "[color=black]";
                            break;
                        }
                    }
                    color
                };
                println!("\"{}\" -> \"{}\" {}", from, to, attrs);
            }
        }
        for sub in p.subs.iter() {
            print_nodes_1(&sub, ignore, highlight, &crates);
        }
    }

    let crates = collect_crate_names(p);
    println!("digraph G {{");
    print_clusters_1(p);
    print_nodes_1(p, ignore_crates, highlight_crates, &crates);
    println!("}}");
}

fn main() {
    let opt = Opt::from_args();

    let all = process_folder(Path::new(".")).expect("Cannot process folder");

    let optional_list = |l: Option<String>| {
        let mut set = StringSet::new();
        if let Some(l) = l {
            for e in l.split(',') {
                set.insert(e.to_string(), Void {});
            }
        }
        set
    };

    let output_format = opt.format.unwrap_or(OutputFormat::Text);
    let ignore_crates = optional_list(opt.ignore);
    let highlight_crates = optional_list(opt.highlight);

    match output_format {
        OutputFormat::Text => print_project_text(&all),
        OutputFormat::Dot => print_project_dot(&all, &ignore_crates, &highlight_crates),
    };
}
