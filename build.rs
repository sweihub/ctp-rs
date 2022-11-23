use lazy_static::lazy_static;
use regex::Regex;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

struct Class {
    name: String,
    methods: Vec<String>,
}

struct Argument {
    class: String,
    name: String,
}

struct Function {
    modifier: String,
    return_type: String,
    name: String,
    args: Vec<Argument>,
    declare: String,
    inputs: String,
}

fn parse_arguments(s: &str) -> Vec<Argument> {
    let mut args: Vec<Argument> = Vec::new();
    if s.is_empty() {
        return args;
    }

    let tokens: Vec<&str> = s.split(",").collect();

    for t in tokens {
        let mut token = t.trim();
        // remove assign expression
        let assign = t.find('=');
        if assign.is_some() {
            token = &t[0..assign.unwrap()];
        }
        token = token.trim();
        // class, name
        let chars: Vec<char> = token.chars().collect();
        let mut i = chars.len() - 1;
        let mut end = chars.len();
        while i > 0 {
            let c = chars[i];
            if c == '=' {
                end = i;
            } else if c == '*' || c == ' ' {
                let class = &token[0..i + 1];
                let name = &token[i + 1..end];
                let arg = Argument {
                    class: class.trim().to_string(),
                    name: name.trim().to_string(),
                };
                args.push(arg);
                break;
            }
            i -= 1;
        }
    }

    return args;
}

fn parse_function(s: &String) -> Function {
    let mut f = Function {
        modifier: "".to_owned(),
        return_type: "".to_owned(),
        name: "".to_owned(),
        args: Vec::new(),
        declare: "".to_owned(),
        inputs: "".to_owned(),
    };

    if s.contains("static ") {
        f.modifier = "static".to_owned();
    } else if s.contains("virtual ") {
        f.modifier = "virtual".to_owned();
    }

    let mut i = s.find(f.modifier.as_str()).unwrap() + f.modifier.len();
    let rest = &s[i..];

    let b1 = rest.find('(').unwrap();
    let b2 = rest.find(')').unwrap();
    let chars: Vec<char> = rest.chars().collect();

    // return type
    // function name
    i = b1;
    while i > 0 {
        let c = chars[i];
        if c == '*' || c == ' ' {
            let name = &rest[i + 1..b1];
            let return_type = &rest[0..i + 1];
            f.name = name.to_string();
            f.return_type = return_type.trim().to_string();
            break;
        }
        i -= 1;
    }

    // args
    let args = &rest[b1 + 1..b2];
    f.args = parse_arguments(args.trim());

    // declare
    let fields: Vec<String> = f
        .args
        .iter()
        .map(|i| format!("{} {}", i.class, i.name))
        .collect();
    f.declare = fields.join(", ");

    // inputs
    let inputs: Vec<String> = f
        .args
        .iter()
        .map(|i| {
            let brace = i.name.find("[]");
            if brace.is_some() {
                let index = brace.unwrap();
                return i.name[0..index].to_string();
            } else {
                return i.name.clone();
            }
        })
        .collect();
    f.inputs = inputs.join(", ");

    return f;
}

fn autogen() -> Result<i32, Box<dyn std::error::Error>> {
    let re_class = Regex::new("class\\s+.*$").unwrap();
    let re_method = Regex::new("\\s*(static|virtual).*;").unwrap();
    let mut classes = Vec::new();
    let headers = [
        "shared/include/ThostFtdcMdApi.h",
        "shared/include/ThostFtdcTraderApi.h",
    ];

    // parse
    for f in headers {
        let file = File::open(f)?;
        let reader = BufReader::new(file);

        let mut class = Class {
            name: "".to_owned(),
            methods: Vec::new(),
        };
        let mut methods = &mut class.methods;

        for i in reader.lines() {
            let line = i?;
            if re_class.is_match(&line) {
                let index = line.rfind(' ').unwrap();
                let name = &line[index + 1..];
                if class.name.is_empty() {
                    class.name = name.to_string();
                } else {
                    // new class, save previous one
                    classes.push(class);
                    class = Class {
                        name: name.to_string(),
                        methods: Vec::new(),
                    };
                    methods = &mut class.methods;
                }
            } else if re_method.is_match(&line) {
                let end = line.rfind(')').unwrap() + 1;
                let method = &line[0..end];
                methods.push(method.to_string());
            }
        }

        if !class.name.is_empty() {
            classes.push(class);
        }
    }

    let mut header = [
        "#pragma warning(disable: 4100)",
        "#pragma once",
        "#include \"../shared/include/DataCollect.h\"",
        "#include \"../shared/include/ThostFtdcUserApiDataType.h\"",
        "#include \"../shared/include/ThostFtdcUserApiStruct.h\"",
        "#include \"../shared/include/ThostFtdcTraderApi.h\"",
        "#include \"../shared/include/ThostFtdcMdApi.h\"",
    ]
    .join("\n");
    let mut body = "#include <iostream>\n#include \"wrapper.hpp\"\n\n".to_owned();

    // walkthrough
    for c in &classes {
        let rust_class = format!("Rust_{}", c.name);

        // implementation
        if c.name == "CThostFtdcMdApi" || c.name == "CThostFtdcTraderApi" {
            header += &format!("\nclass {} {{\npublic:\n", rust_class);

            // C++ side
            for method in &c.methods {
                let f = parse_function(method);
                if f.modifier == "static" {
                    // header
                    if f.name == "CreateFtdcTraderApi" || f.name == "CreateFtdcMdApi" {
                        let line = format!("\tstatic {}* {}({});\n", rust_class, f.name, f.declare);
                        header += &line;
                        // body
                        let source = format!(
                            "{}* {}::{}({}) {{ 
                            {} * self = new {}();
                            self->inner = {}::{}({});                        
                            return self;
                        }}\n",
                            rust_class,
                            rust_class,
                            f.name,
                            f.declare,
                            rust_class,
                            rust_class,
                            c.name,
                            f.name,
                            f.inputs
                        );
                        body += &source;
                    } else {
                        header +=
                            &format!("\tstatic {} {}({});\n", f.return_type, f.name, f.declare);
                        // body
                        let source = format!(
                            "{} {}::{}({}) {{ return {}::{}({}); }}\n",
                            f.return_type, rust_class, f.name, f.declare, c.name, f.name, f.inputs
                        );
                        body += &source;
                    }
                } else {
                    let line = format!("\t{} {}({});\n", f.return_type, f.name, f.declare);
                    header += &line;
                    // body
                    let source = format!(
                        "{} {}::{}({}) {{ return inner->{}({}); }}\n",
                        f.return_type, rust_class, f.name, f.declare, f.name, f.inputs
                    );
                    body += &source;
                }
            }
            header += &format!("private:\n\t{} * inner = nullptr;\n", c.name);
            header += "};\n\n";
        } else if c.name == "CThostFtdcMdSpi" || c.name == "CThostFtdcTraderSpi" {
            header += &format!("\nclass {} : {} {{\npublic:\n", rust_class, c.name);
            // C++ upcall to Rust
            // Create
            let consructor = format!("\tstatic {}* Create(void * trait);\n", c.name);
            header += &consructor;
            body += &format!(
                "{}* {}::Create(void * trait) {{ 
                {}* p = new {}();
                p->rust = trait;
                return p;
            }}\n",
                c.name, rust_class, rust_class, rust_class
            );

            // Destroy
            header += &format!("\tstatic void Destroy({}* ptr);\n", c.name);
            body += &format!("void {}::Destroy({}* ptr) {{ delete ptr; }}\n", rust_class, c.name);

            let mut upcall = "".to_owned();
            for method in &c.methods {
                let f = parse_function(method);
                let line = format!("\t{} {}({});\n", f.return_type, f.name, f.declare);
                header += &line;
                // upcall declare
                let rust = if f.inputs.is_empty() {
                    "void * rust"
                } else {
                    "void * rust, "
                };
                upcall += &format!(
                    "extern \"C\" {} Rust_{}_Trait_{}({}{});\n",
                    f.return_type, c.name, f.name, rust, f.declare
                );
                // body
                let rust = if f.inputs.is_empty() {
                    "rust"
                } else {
                    "rust, "
                };
                let source = format!(
                    "{} {}::{}({}) {{ return Rust_{}_Trait_{}({}{}); }}\n",
                    f.return_type, rust_class, f.name, f.declare, c.name, f.name, rust, f.inputs
                );
                body += &source;
            }

            header += &format!("private:\n\tvoid * rust = nullptr;\n");
            header += "};\n\n";
            header += &upcall;
        }
    }

    //println!("{}", header);
    //println!("{}", body);
    std::fs::write("src/wrapper.hpp", header)?;
    std::fs::write("src/wrapper.cpp", body)?;

    return Ok(0);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    // remove the wrapper to generate again
    if !Path::new("src/wrapper.hpp").exists() {
        autogen()?;
        //panic!("HERE");
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let platform = if cfg!(target_family = "windows") {
        "windows"
    } else {
        "unix"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        panic!("can not build on this platform.")
    };

    cc::Build::new()
        .cpp(true)
        .file("src/wrapper.cpp")
        .flag_if_supported("-std=c++17")
        .flag_if_supported("-w")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("wrapper");

    println!(
        "cargo:rustc-link-search={}",
        root.join("shared/md")
            .join(format!("{}.{}", platform, arch))
            .display()
    );
    println!(
        "cargo:rustc-link-search={}",
        root.join("shared/td")
            .join(format!("{}.{}", platform, arch))
            .display()
    );
    println!(
        "cargo:rustc-link-search={}",
        root.join("shared/data_collect")
            .join(format!("{}.{}", platform, arch))
            .display()
    );

    //println!("{}", root.display().to_string());
    //panic!("DEBUG");

    if platform == "unix" {
        println!("cargo:rustc-link-lib=dylib=LinuxDataCollect");
    } else {
        println!("cargo:rustc-link-lib=dylib=WinDataCollect");
    }
    println!("cargo:rustc-link-lib=dylib=thostmduserapi_se");
    println!("cargo:rustc-link-lib=dylib=thosttraderapi_se");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=src/wrapper.hpp");
    println!("cargo:rerun-if-changed=src/wrapper.cpp");

    // ctp api header is clean enough, we will use blacklist instead whitelist
    let bindings = bindgen::Builder::default()
        .clang_arg("-xc++")
        .clang_arg("-std=c++17")
        // The input header we would like to generate
        // bindings for.
        .header("src/wrapper.hpp")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .derive_debug(true)
        // make output smaller
        .layout_tests(false)
        .generate_comments(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // we will handle class mannually by `autobind.py`
        // function defined in rust
        .opaque_type("CThostFtdcTraderApi")
        .opaque_type("CThostFtdcTraderSpi")
        .opaque_type("CThostFtdcMdApi")
        .opaque_type("CThostFtdcMdSpi")
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    // let outfile = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");

    // NOTE:
    // binding_gen.rs was slightly modified to bindings.rs in order to silence warnings,
    // if you change the wrapper.cpp, please manually move bindings_gen.rs to bingdings.rs
    let outfile = root.join("src/sys/bindings.rs");

    bindings
        .write_to_file(&outfile)
        .expect("Couldn't write bindings!");

    let buf = replace_trait(
        &outfile,
        &[
            "Rust_CThostFtdcMdSpi_Trait",
            "Rust_CThostFtdcTraderSpi_Trait",
        ],
    )
    .expect("Fail to replace trait!");
    std::fs::write(&outfile, &buf).expect("Fail to write converted bindings!");

    Ok(())
}

fn camel_to_snake<'t>(name: &'t str) -> String {
    lazy_static! {
        static ref PATTERN1: Regex = Regex::new(r"(.)([A-Z][a-z]+)").unwrap();
        static ref PATTERN2: Regex = Regex::new(r"([a-z0-9])([A-Z])").unwrap();
    }
    PATTERN2
        .replace_all(
            PATTERN1.replace_all(name, r"${1}_${2}").as_ref(),
            r"${1}_${2}",
        )
        .to_lowercase()
}

fn replace_trait(fname: &Path, traits: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let mut buf = std::fs::read_to_string(fname)?;
    for trait_extern in traits {
        let pattern = Regex::new(&format!(
            r#"extern \s*"C"\s*\{{\s*pub\s+fn\s+{}_(\w+)\s*\(([^)]*)\)([^;]*);\s*}}\s*"#,
            trait_extern
        ))
        .unwrap();
        let pattern_arg = Regex::new(r"\s*(\w+)\s*:\s*(.*)\s*").unwrap();

        let mut exports = vec![];
        let mut traitfuns = vec![];

        assert!(
            pattern.captures(&buf).is_some(),
            "`{}` not found in source code",
            trait_extern
        );

        for cap in pattern.captures_iter(&buf) {
            let fname = cap.get(1).unwrap().as_str().trim();
            let args: Vec<_> = cap
                .get(2)
                .unwrap()
                .as_str()
                .split(",")
                .filter(|s| s.trim().len() > 0)
                .map(|s| {
                    let c = pattern_arg.captures(s).unwrap();
                    (c.get(1).unwrap().as_str(), c.get(2).unwrap().as_str())
                })
                .collect();
            let rtn = cap.get(3).unwrap().as_str();
            let fname_camel = camel_to_snake(fname);
            if fname_camel == "drop" {
                continue;
            }
            assert!(args[0].1.trim().ends_with("c_void"));

            let mut tmp = args[1..]
                .iter()
                .map(|s| format!("{}: {}", s.0, s.1))
                .collect::<Vec<_>>();
            tmp.insert(0, "trait_ptr: *mut ::std::os::raw::c_void".into());
            let args_repl = tmp.join(", ");
            let argv_repl = args[1..].iter().map(|s| s.0).collect::<Vec<_>>().join(", ");

            let export = format!(
                r#"#[no_mangle]
pub extern "C" fn {trait_extern}_{fname}({args_repl}){rtn} {{
    let ptr = trait_ptr as *mut Box<dyn {trait_extern}>;
    let trait_obj: &mut dyn {trait_extern} = unsafe {{ &mut **ptr }};
    trait_obj.{fname_camel}({argv_repl})
}}
"#,
                trait_extern = trait_extern,
                fname = fname,
                args_repl = args_repl,
                rtn = rtn,
                fname_camel = fname_camel,
                argv_repl = argv_repl
            );
            exports.push(export);

            let mut tmp = args[1..]
                .iter()
                .map(|s| format!("{}: {}", s.0, s.1))
                .collect::<Vec<_>>();
            tmp.insert(0, "&mut self".into());
            let args_repl = tmp.join(", ");
            let traitfun = format!(
                r"    fn {fname_camel}({args_repl}){rtn} {{  }}",
                fname_camel = fname_camel,
                args_repl = args_repl,
                rtn = rtn
            );
            traitfuns.push(traitfun);
        }

        let exports_repl = exports.join("\n");
        let traitfuns_repl = traitfuns.join("\n");

        buf = format!(
            r#"{ori}
#[allow(unused)]
pub trait {trait_extern} {{
{traitfuns_repl}
}}

{exports_repl}
#[no_mangle]
pub extern "C" fn {trait_extern}_Drop(trait_obj: *mut ::std::os::raw::c_void) {{
    let trait_obj = trait_obj as *mut Box<dyn {trait_extern}>;
    let _r: Box<Box<dyn {trait_extern}>> = unsafe {{ Box::from_raw(trait_obj) }};
}}
"#,
            ori = pattern.replace_all(&buf, "").to_string(),
            exports_repl = exports_repl,
            trait_extern = trait_extern,
            traitfuns_repl = traitfuns_repl
        );
    }

    Ok(buf)
}
