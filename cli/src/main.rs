use clap::{App, AppSettings, Arg, SubCommand};
use error::Error;
use std::path::Path;
use std::process::exit;
use std::{env, fs, u64};
use tree_sitter_cli::{
    config, error, generate, highlight, loader, logger, parse, test, wasm, web_ui,
};

const BUILD_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const BUILD_SHA: Option<&'static str> = option_env!("BUILD_SHA");

fn main() {
    if let Err(e) = run() {
        if !e.message().is_empty() {
            println!("");
            eprintln!("{}", e.message());
        }
        exit(1);
    }
}

fn run() -> error::Result<()> {
    let version = if let Some(build_sha) = BUILD_SHA {
        format!("{} ({})", BUILD_VERSION, build_sha)
    } else {
        BUILD_VERSION.to_string()
    };

    let matches = App::new("tree-sitter")
        .version(version.as_str())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .author("Max Brunsfeld <maxbrunsfeld@gmail.com>")
        .about("Generates and tests parsers")
        .subcommand(SubCommand::with_name("init-config").about("Generate a default config file"))
        .subcommand(
            SubCommand::with_name("generate")
                .about("Generate a parser")
                .arg(Arg::with_name("grammar-path").index(1))
                .arg(Arg::with_name("log").long("log"))
                .arg(Arg::with_name("properties-only").long("properties"))
                .arg(Arg::with_name("no-minimize").long("no-minimize")),
        )
        .subcommand(
            SubCommand::with_name("parse")
                .about("Parse a file")
                .arg(
                    Arg::with_name("path")
                        .index(1)
                        .multiple(true)
                        .required(true),
                )
                .arg(Arg::with_name("scope").long("scope").takes_value(true))
                .arg(Arg::with_name("debug").long("debug").short("d"))
                .arg(Arg::with_name("debug-graph").long("debug-graph").short("D"))
                .arg(Arg::with_name("quiet").long("quiet").short("q"))
                .arg(Arg::with_name("time").long("time").short("t"))
                .arg(Arg::with_name("allow-cancellation").long("cancel"))
                .arg(Arg::with_name("timeout").long("timeout").takes_value(true))
                .arg(
                    Arg::with_name("edits")
                        .long("edit")
                        .short("edit")
                        .takes_value(true)
                        .multiple(true)
                        .number_of_values(1),
                ),
        )
        .subcommand(
            SubCommand::with_name("test")
                .about("Run a parser's tests")
                .arg(
                    Arg::with_name("filter")
                        .long("filter")
                        .short("f")
                        .takes_value(true),
                )
                .arg(Arg::with_name("debug").long("debug").short("d"))
                .arg(Arg::with_name("debug-graph").long("debug-graph").short("D")),
        )
        .subcommand(
            SubCommand::with_name("highlight")
                .about("Highlight a file")
                .arg(
                    Arg::with_name("path")
                        .index(1)
                        .multiple(true)
                        .required(true),
                )
                .arg(Arg::with_name("scope").long("scope").takes_value(true))
                .arg(Arg::with_name("html").long("html").short("h"))
                .arg(Arg::with_name("time").long("time").short("t")),
        )
        .subcommand(
            SubCommand::with_name("build-wasm")
                .about("Compile a parser to WASM")
                .arg(
                    Arg::with_name("docker")
                        .long("docker")
                        .help("Run emscripten via docker even if it is installed locally"),
                )
                .arg(Arg::with_name("path").index(1).multiple(true)),
        )
        .subcommand(
            SubCommand::with_name("web-ui").about("Test a parser interactively in the browser"),
        )
        .subcommand(
            SubCommand::with_name("dump-languages")
                .about("Print info about all known language parsers"),
        )
        .get_matches();

    let home_dir = dirs::home_dir().expect("Failed to read home directory");
    let current_dir = env::current_dir().unwrap();
    let config = config::Config::load(&home_dir);
    let mut loader = loader::Loader::new(config.binary_directory.clone());

    if matches.subcommand_matches("init-config").is_some() {
        let config = config::Config::new(&home_dir);
        config.save(&home_dir)?;
    } else if let Some(matches) = matches.subcommand_matches("generate") {
        let grammar_path = matches.value_of("grammar-path");
        let properties_only = matches.is_present("properties-only");
        if matches.is_present("log") {
            logger::init();
        }
        generate::generate_parser_in_directory(&current_dir, grammar_path, properties_only)?;
    } else if let Some(matches) = matches.subcommand_matches("test") {
        let debug = matches.is_present("debug");
        let debug_graph = matches.is_present("debug-graph");
        let filter = matches.value_of("filter");
        let corpus_path = current_dir.join("corpus");
        if let Some(language) = loader.languages_at_path(&current_dir)?.first() {
            test::run_tests_at_path(*language, &corpus_path, debug, debug_graph, filter)?;
        } else {
            eprintln!("No language found");
        }
    } else if let Some(matches) = matches.subcommand_matches("parse") {
        let debug = matches.is_present("debug");
        let debug_graph = matches.is_present("debug-graph");
        let quiet = matches.is_present("quiet");
        let time = matches.is_present("time");
        let edits = matches
            .values_of("edits")
            .map_or(Vec::new(), |e| e.collect());
        let allow_cancellation = matches.is_present("allow-cancellation");
        let timeout = matches
            .value_of("timeout")
            .map_or(0, |t| u64::from_str_radix(t, 10).unwrap());
        loader.find_all_languages(&config.parser_directories)?;
        let paths = matches
            .values_of("path")
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>();
        let max_path_length = paths.iter().map(|p| p.chars().count()).max().unwrap();
        let mut has_error = false;
        for path in paths {
            let path = Path::new(path);
            let language = if let Some(scope) = matches.value_of("scope") {
                if let Some(config) =
                    loader
                        .language_configuration_for_scope(scope)
                        .map_err(Error::wrap(|| {
                            format!("Failed to load language for scope '{}'", scope)
                        }))?
                {
                    config.0
                } else {
                    return Error::err(format!("Unknown scope '{}'", scope));
                }
            } else if let Some((lang, _)) = loader
                .language_configuration_for_file_name(path)
                .map_err(Error::wrap(|| {
                    format!(
                        "Failed to load language for file name {:?}",
                        path.file_name().unwrap()
                    )
                }))?
            {
                lang
            } else if let Some(lang) = loader
                .languages_at_path(&current_dir)
                .map_err(Error::wrap(|| {
                    "Failed to load language in current directory"
                }))?
                .first()
                .cloned()
            {
                lang
            } else {
                eprintln!("No language found");
                return Ok(());
            };
            has_error |= parse::parse_file_at_path(
                language,
                path,
                &edits,
                max_path_length,
                quiet,
                time,
                timeout,
                debug,
                debug_graph,
                allow_cancellation,
            )?;
        }

        if has_error {
            return Error::err(String::new());
        }
    } else if let Some(matches) = matches.subcommand_matches("highlight") {
        let paths = matches.values_of("path").unwrap().into_iter();
        let html_mode = matches.is_present("html");
        let time = matches.is_present("time");
        loader.find_all_languages(&config.parser_directories)?;

        if html_mode {
            println!("{}", highlight::HTML_HEADER);
        }

        let language_config;
        if let Some(scope) = matches.value_of("scope") {
            language_config = loader.language_configuration_for_scope(scope)?;
            if language_config.is_none() {
                return Error::err(format!("Unknown scope '{}'", scope));
            }
        } else {
            language_config = None;
        }

        for path in paths {
            let path = Path::new(path);
            let (language, language_config) = match language_config {
                Some(v) => v,
                None => match loader.language_configuration_for_file_name(path)? {
                    Some(v) => v,
                    None => {
                        eprintln!("No language found for path {:?}", path);
                        continue;
                    }
                },
            };

            if let Some(sheet) = language_config.highlight_property_sheet(language)? {
                let source = fs::read(path)?;
                if html_mode {
                    highlight::html(&loader, &config.theme, &source, language, sheet)?;
                } else {
                    highlight::ansi(&loader, &config.theme, &source, language, sheet, time)?;
                }
            } else {
                return Error::err(format!("No syntax highlighting property sheet specified"));
            }
        }
    } else if let Some(matches) = matches.subcommand_matches("build-wasm") {
        let grammar_path = current_dir.join(matches.value_of("path").unwrap_or(""));
        wasm::compile_language_to_wasm(&grammar_path, matches.is_present("docker"))?;
    } else if matches.subcommand_matches("web-ui").is_some() {
        web_ui::serve(&current_dir);
    } else if matches.subcommand_matches("dump-languages").is_some() {
        loader.find_all_languages(&config.parser_directories)?;
        for (configuration, language_path) in loader.get_all_language_configurations() {
            println!(
                "scope: {}\nparser: {:?}\nproperties: {:?}\nfile_types: {:?}\ncontent_regex: {:?}\ninjection_regex: {:?}\n",
                configuration.scope.as_ref().unwrap_or(&String::new()),
                language_path,
                configuration.highlight_property_sheet_path,
                configuration.file_types,
                configuration.content_regex,
                configuration.injection_regex,
            );
        }
    }

    Ok(())
}
