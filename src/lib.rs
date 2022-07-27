/*- Global allowings -*/
#![allow(
    unused_variables,

    // Wasm_bindgen sometimes complains about
    // non-uppercase when it actually shouldn't be
    non_upper_case_globals,
    dead_code,
    unused_imports
)]

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/*- Imports -*/
use js_sys::{self, Math::pow};
use reqwest;
use regex::{Regex, Captures};
use std::{sync::Mutex, num, future::Future, pin::Pin, str::ParseBoolError};
use lazy_static::lazy_static;
use wasm_bindgen::prelude::*;
use std::collections::HashMap;

/*- Mutable global arrays -*/
lazy_static! {
    static ref VARIABLES:Mutex<Vec<(String, String)>> = Mutex::new(vec![]);
    static ref FUNCTIONS:Mutex<Vec<(String, String, Vec<String>)>> = Mutex::new(vec![]);
}

/*- Commands are listed here -*/
const COMMANDS:&[(&str, fn(Vec<&str>) -> String, &str); 17] = &[
    ("return", _return, "Print text to the terminal. Example: |return hello world!|"),
    ("repeat", _repeat, "Repeat commands x number of times. Example: |repeat 10 i return index: #i|"),
    ("help", _help, "|help| will list all commands. |help command_name| will give a description of how you use that command."),
    ("set", _set, "Set a variable. |set variable_name variable_value|"),
    ("get", _get, "Get a variable. |get variable_name|"),
    ("ol", _ol, "Runs commands, but makes their output one-line. Example: |ol repeat 15 return hello|"),
    ("olc", _olc, "Runs commands, but makes their output one-line, without spaces. Example: |ol repeat 15 return hello|"),
    ("fn", _fn, "Create a function. Example: |fn function_name(param1,param2) return p1: --param1, p2: --param2|"),
    ("exec", _exec, "Execute a function. Example: |exec function_name(param1,param2)|"),
    ("list", _list, "List global variables. Example: |list vars|, |list cmd|, |list fn|"),
    ("replace", _replace, "Replace strings inside of a string. Example: |replace hello lo loooo|, |replace hi hi :space: :nothing:|"),
    ("random", _random, "Get a random number. Example: |random 1 100|"),
    ("calc", _calc, "Calculate things. Example: |calc 5 * 2 + 1 - 4 / 5|"),
    ("if", _if, "Execute a commands depending on a condition. Example: |if (eval(calc 5 * 5) == 25) {return yes} else {return this will never be called}|"),

    // These functions are defined in the js-side.
    ("reset", |name| { String::new() }, "[JS-SIDE] Clears the terminal. Variables are still kept."),
    ("theme", |name| { String::new() }, "[JS-SIDE] Changes the theme. Example: |theme aqua|"),
    ("full",  |name| { String::new() }, "[JS-SIDE] Toggles fullscreen."),
];

/*- Call commands -*/
#[wasm_bindgen]
pub fn command(input:String) -> String {
    let mut output:Vec<String> = Vec::new();
    let mut fn_found:bool = false;

    /*- A command can contain '&&' which will make users
        be able to execute multiple commands in one line -*/
    for command_ in input.split("&&") {

        /*- Get the command name -*/
        let command_name = command_.split_whitespace().nth(0).unwrap_or("");
        
        /*- Find the command and call it -*/
        'inner: for (command, caller, _) in COMMANDS {
            if &command_name == command {
                
                /*- Get the input -*/
                let command_ = replace_info(command_.to_string());
                
                /*- Get the args -*/
                let argv:Vec<&str> = command_.split_whitespace().skip(1).collect();
                
                /*- Call the function -*/
                output.push(caller(argv));
                fn_found = true;
                break 'inner;
            };
        };

        // /*- Get the possible function name -*/
        // let fn_name_regex:Regex = Regex::new(r"(.+?)\(").unwrap().captures(command_name).unwrap().get(1).unwrap().as_str().parse().unwrap();

        // /*- If no builtin-command was found, check for user-created functions -*/
        // for (fn_name, _, __) in FUNCTIONS.lock().unwrap().iter() {
        //     if fn_name == command_name {
        //         output.push(_exec(command_.split_whitespace().collect()));
        //     }
        // }
    };

    /*- Return -*/
    if fn_found { return output.join("<br />"); };
    "Command not found!".to_string()
}

/*- Replace info is a function that replaces
    things like :date: with the actual date -*/
#[wasm_bindgen]
pub fn replace_info(input:String) -> String {

    /*- Quick replacements -*/
    let mut input = input
        .replace("\\n", "<br />")
        .replace("\\_", " ");

    /*- Regex for random number gen, can be called like this - :random 10-124: which will replace the input with something like 23 -*/
    let random_regex = Regex::new(r":random\s([0-9]+)-([0-9]+):").unwrap();

    /*- Typing 'var(variable_name)' will replace it with the value -*/
    let var_regex = Regex::new(r"var\((.+?)\)").unwrap();

    /*- Typing 'replace(string,replace,with)' will replace all 'replace' with 'with' -*/
    let replace_regex = Regex::new(r"replace\((.+?)\)").unwrap();

    /*- Replace all variables -*/
    for (k, v) in VARIABLES.lock().unwrap().clone() {
        input = input.replace(
            &format!("<{k}>"),
            &v
        );
    };
    
    /*- Make the random replacement -*/
    let input = random_regex.replace_all(&input, |caps: &regex::Captures| {
        let min = caps.get(1).unwrap().as_str().parse::<i32>().unwrap();
        let max = caps.get(2).unwrap().as_str().parse::<i32>().unwrap();
        let random = (js_sys::Math::random() * (max - min)as f64 + min as f64) as i32;
        random.to_string()
    }).to_string();

    /*- Make the 'var' replacement -*/
    let input = var_regex.replace_all(&input, |caps: &regex::Captures| {
        /*- The 0:th capture is the whole thing, the 1:st one is the command -*/
        match caps.get(1) {
            Some(capture) => variable(capture.as_str()),
            None => String::new()
        }
    }).to_string();

    /*- Make the 'replace' replacement -*/
    let input = replace_regex.replace_all(&input, |caps: &regex::Captures| {
        /*- The 0:th capture is the whole thing, the 1:st one is the command -*/
        match caps.get(1) {
            Some(capture) => {
                let caps_:Vec<&str> = capture.as_str().split(",").collect();
                if caps_.len() < 3 { return String::from("Invalid replace command!") };

                (caps_[0].replace(caps_[1], caps_[2])).to_string()
            },
            None => "".to_string()
        }
    }).to_string();

    /*- Return -*/
    input
}

fn eval_string(input:String) -> String {

    /*- Typing 'eval(some_command)' will replace it with the output of the command -*/
    let eval_regex = Regex::new(r"eval\((.+?)\)").unwrap();

    /*- Make the 'eval' replacement -*/
    let input = eval_regex.replace_all(&input, |caps: &regex::Captures| {
        /*- The 0:th capture is the whole thing, the 1:st one is the command -*/
        command(match caps.get(1) {
            Some(capture) => capture.as_str().to_string(),
            None => String::new()
        })
    }).to_string();

    input
}

/*- All commands -*/
/// Print something to stdout
pub fn _return(input:Vec<&str>) -> String {
    return
        input.join(" ");
}

/// Repeat some code
pub fn _repeat(input:Vec<&str>) -> String {
    /*- The amount of times the code will repeat -*/
    /*- If num of repeat was specified -*/
    let num_of_repeat:i32 = match input.get(0) {
        Some(num) => num.parse::<i32>().unwrap_or(1),
        None => return String::from("Num-repeat not specified! Type |help repeat| for further info.")
    };

    /*- Get what the user wants to name the index -*/
    let index_name:&str = match input.get(1) {
        Some(name) => name,
        None => return String::from("Index not specified! Type |help repeat| for further info.")
    };

    /*- Check if the command was specified -*/
    if input.len() <= 2 { return String::from("No command to repeat was specified! Type |help repeat| for further info.") };

    /*- Get the command and its arguments -*/
    let _command = &input[2..].join(" ");

    /*- The output of all commands -*/
    let mut output:Vec<String> = Vec::with_capacity(num_of_repeat as usize);

    /*- Repeat the command -*/
    for i in 0..num_of_repeat {
        output.push(
            command(
                /*- We'll replace the --i flag with the index -*/
                eval_string(_command.replace(
                    &format!(
                        "#{}",
                        index_name
                    ),
                    &i.to_string()
                )).to_string()
            )
        )
    };

    output.join("<br />")
}

/// Help with commands
pub fn _help(input:Vec<&str>) -> String {
    let mut out = Vec::new();

    /*- See if the user wants a description of a command -*/
    match input.get(0) {
        Some(command_name) => {
            /*- Find the command -*/
            for (name, _, description) in COMMANDS {
                if name == command_name {
                    return description.to_string();
                }
            };

            return format!("No such command: '{command_name}'");
        },
        None => {
            /*- Get all command names -*/
            for (command_name, _, __) in COMMANDS {
                out.push(*command_name);
            };
        }
    }

    /*- Return -*/
    format!(
        "{}<br />{}",
        "Type |help command_name| for further info on each command",
        out.join(" - "),
    )
}

// Get variables
pub fn _get(input:Vec<&str>) -> String {
    let variable_name = &input.get(0).unwrap_or(&"");

    /*- Get the variable value -*/
    variable(variable_name)
}

// Set variables
pub fn _set(input:Vec<&str>) -> String {

    /*- Set the variable -*/
    VARIABLES.lock().unwrap().push((
        input.get(0).unwrap_or(&"").to_string(),
        input.get(1).unwrap_or(&"").to_string(),
    ));

    /*- Return success -*/
    String::from("Success")
}

// Command with one-line output
pub fn _ol(input:Vec<&str>) -> String {
    let output = command(input.join(" "))
                .replace("<br />", " ")
                .replace("\n", "");
    
    output
}
// Command with one-line output (without spaces)
pub fn _olc(input:Vec<&str>) -> String {
    let output = command(input.join(" "))
                .replace("<br />", "")
                .replace("\n", "");
    
    output
}

// Create function
pub fn _fn(input:Vec<&str>) -> String {
    /*- Get the function name -*/
    /*- If fnname was specified -*/
    let fn_name:String = match input.get(0) {
        Some(name) => name.to_string(),
        None => return String::from("Function name not specified! Type |help fn| for further info.")
    };

    /*- fn_name will look like this: name(params),
        so we'll extract the params from the name -*/
    let name_regex:Regex = Regex::new(r"(.+?)\((.*?|)\)").unwrap();
    let name_captures = match name_regex.captures(&fn_name) {
        Some(n) => n,
        None => return String::from("Invalid fn declaration! Type |help fn| for further info.")
    };
    
    let (fn_name, params): (String, Vec<String>) = (
        match name_captures.get(1) {
            Some(string) => string.as_str().to_string(),
            None => return String::from("Invalid fn declaration! Type |help fn| for further info.")
        },
        match name_captures.get(2) {
            Some(string) => string.as_str().split(",").map(|e| e.trim().to_string()).collect::<Vec<String>>(),
            None => return String::from("Invalid fn declaration! Type |help fn| for further info.")
        }        
    );

    /*- Check if function-name is reserved -*/
    for (name, _, __) in COMMANDS {
        if name == &fn_name {
            return format!("Function name '{}' is reserved!", fn_name);
        };
    };

    /*- Check if the command was specified -*/
    if input.len() <= 1 { return String::from("No command was specified! Type |help fn| for further info.") };

    /*- Get the command and its arguments -*/
    let _command = &input[1..].join(" ");
    let _command = _command.replace("__AND__", "&&");

    /*- Set the functiom -*/
    FUNCTIONS.lock().unwrap().push(( fn_name, _command.to_string(), params ));

    String::from("Success!")
}

// Call function
pub fn _exec(input:Vec<&str>) -> String {
    let function = &input.get(0).unwrap_or(&"");
    let name_regex:Regex = Regex::new(r"(.+?)\((.*?|)\)").unwrap();
    log("1");
    let name_captures = match name_regex.captures(&function) {
        Some(n) => n,
        None => return String::from("Invalid fn declaration! Type |help fn| for further info.")
    };
    let (fn_name, params): (String, Vec<String>) = (
        match name_captures.get(1) {
            Some(string) => string.as_str().to_string(),
            None => return String::from("Invalid exec declaration! Type |help exec| for further info.")
        },
        match name_captures.get(2) {
            Some(string) => string.as_str().split(",").map(|e| eval_string(e.trim().to_string())).collect::<Vec<String>>(),
            None => return String::from("Invalid exec declaration! Type |help exec| for further info.")
        }  
    );


    /*- Get the variable -*/
    for (k, v, p) in FUNCTIONS.lock().unwrap().clone() {
        log("Search");
        if k == fn_name {

            log("3.1");

            /*- Get the function arguments -*/
            let mut final_command:String = v;

            log("3.2");

            /*- Replace all params -*/
            for (index, arg) in p.iter().enumerate() {
                let param_value = params.get(index).unwrap_or(&String::new()).clone();
                log("3.3");
                final_command = final_command.replace(
                    &format!("--{arg}"), &param_value
                );
                log("3.4");
            };
            log("3.5");

            return command(
                final_command
            );
        };
    };
    log("4");

    /*- Return else -*/
    String::from("null")
}

// List globals
pub fn _list(input:Vec<&str>) -> String {
    let what_to_list = &input.get(0).unwrap_or(&"");

    /*- Check what the user wants to list -*/
    match what_to_list {
        &&"var" => {
            return VARIABLES
                .lock() // Get the array from the mutex guard
                .unwrap()
                .clone()
                .into_iter() // Make it an iterator
                .map(|(e, v)| e) // Get the key from the tuple
                .collect::<Vec<String>>() // Make it into an array
                .join(" | "); // Make it into a string
        },
        &&"fn" => {
            return FUNCTIONS
                .lock() // Get the array from the mutex guard
                .unwrap()
                .clone()
                .into_iter() // Make it an iterator
                .map(|(e, v, _)| e) // Get the key from the tuple
                .collect::<Vec<String>>() // Make it into an array
                .join(" | "); // Make it into a string
        },
        &&"cmd" => {
            return COMMANDS
                .clone()
                .into_iter() // Make it an iterator
                .map(|(e, _, __)| e) // Get the key from the tuple
                .collect::<Vec<&str>>() // Make it into an array
                .join(" | "); // Make it into a string
        },
        _ => return String::from("Couldn't list that. Type |help list| for further info.")
    };
}

// Replace things in strings
pub fn _replace(input:Vec<&str>) -> String {
    let a = eval_string(input.get(0..input.len()-2).unwrap_or_default().join(" "));


    let (string, replace, with) = (
        a,
        input.get(input.len()-2),
        input.get(input.len()-1),
    );

    /*- Check the availability of all params -*/
    // let string = match string { Some(s) => s, None => return String::from("String to replace not specified. Type |help replace| for more info.") };
    let replace = match replace { Some(s) => s, None => return String::from("Character to replace not specified. Type |help replace| for more info.") };
    let with = match with { Some(s) => s, None => return String::from("What to replace not specified. Type |help replace| for more info.") };

    /*- Return -*/
    if with == &":nothing:" {
        if replace == &":space:" {
            string.replace(" ", "")
        }else {
            string.replace(replace, "")
        }
    }else {
        if replace == &":space:" {
            string.replace(" ", with)
        }else {
            string.replace(replace, with)
        }
    }
}

// Random number generator
pub fn _random(input:Vec<&str>) -> String {
    let (min, max) = (
        input.get(0),
        input.get(1),
    );

    /*- Check the availability of all params -*/
    let min = match min { Some(s) => s.parse::<i32>().unwrap_or(0i32), None => return String::from("Minimum val not specified.") };
    let max = match max { Some(s) => s.parse::<i32>().unwrap_or(0i32), None => return String::from("Maximum val not specified.") };

    /*- Return -*/
    ((js_sys::Math::random() * (max - min)as f64 + min as f64) as i32).to_string()
}

// Calculate numbers
pub fn _calc(input:Vec<&str>) -> String {
    let input:String = eval_string(input.join(" ")).replace(" ", "");

    /*- All regexes -*/
    let power_re =          Regex::new(r"([0-9\.]+)!([0-9\.]+)").unwrap();
    let multiplication_re = Regex::new(r"([0-9\.]+)\*([0-9\.]+)").unwrap();
    let divide_re =         Regex::new(r"([0-9\.]+)/([0-9\.]+)").unwrap();
    let addition_re =       Regex::new(r"([0-9\.]+)\+([0-9\.]+)").unwrap();
    let subtraction_re =    Regex::new(r"([0-9\.]+)\-([0-9\.]+)").unwrap();

    /*- Replace power -*/
    let input = power_re.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")})
        );

        pow(n1 as f64, n2 as f64).to_string()
    }).to_string();

    /*- Replace multiplications -*/
    let input = multiplication_re.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")})
        );

        (n1*n2).to_string()
    }).to_string();

    /*- Replace divisions -*/
    let input = divide_re.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")})
        );

        (n1/n2).to_string()
    }).to_string();

    /*- Replace additions -*/
    let input = addition_re.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")})
        );

        (n1+n2).to_string()
    }).to_string();

    /*- Replace subtractions -*/
    let input = subtraction_re.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing calculation")})
        );

        (n1-n2).to_string()
    }).to_string();

    /*- Return -*/
    input
}

// Create if-statements
pub fn _if(input:Vec<&str>) -> String {
    let input = input.join(" ");

    /*- Get the condition -*/
    let condition_re:Regex = Regex::new(r"\((.+)\) \{(.+)\} else \{(.+)\}").unwrap();
    condition_re.replace(&input, "");
    
    let (condition, _do, _else) = match condition_re.captures(&input) {
        Some(s) => (
            match s.get(1) { Some(e) => e.as_str().to_string(), None => return String::from("Couldn't regex that."), },
            match s.get(2) { Some(e) => e.as_str().to_string(), None => return String::from("Nothing to do if condition succeeds."), },
            match s.get(3) { Some(e) => e.as_str().to_string(), None => return String::from("ELse shit."), },
        ),
        None => return String::from("Condition not found")
    };

    /*- Get if the condition is true / false -*/
    let condition:bool = match parse_condition(condition) {
        Ok(s) => s,
        Err(_) => return String::from("Error parsing condition")
    };

    /*- Execute the command -*/
    if condition {
        command(_do)
    }else {
        command(_else)
    }
}


// Helper functions
fn variable(variable_name:&str) -> String {

    /*- Get the variable -*/
    for (k, v) in VARIABLES.lock().unwrap().clone() {
        if &&k == &variable_name {
            return v;
        };
    };

    /*- Return else -*/
    String::from("null")
}

fn parse_num(input:String) -> f32 {
    input.parse::<f32>().unwrap_or(0f32)
}

fn parse_condition(input:String) -> Result<bool, ParseBoolError> {
    let input:String = eval_string(input).replace(" ", "");

    /*- All regexes -*/
    let bigger_than =   Regex::new(r"([0-9\.]+)>([0-9\.]+)").unwrap();
    let less_than =     Regex::new(r"([0-9\.]+)<([0-9\.]+)").unwrap();
    let modulo =        Regex::new(r"([0-9\.]+)%([0-9\.]+)").unwrap();
    let equals =        Regex::new(r"(.+)==(.+)").unwrap();

    /*- Replace modulo -*/
    let input = modulo.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing number.")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing number.")})
        );

        (n1 % n2).to_string()
    });

    /*- Replace bigger_than -*/
    let input = bigger_than.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing number.")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing number.")})
        );

        (n1 > n2).to_string()
    });

    /*- Replace less than -*/
    let input = less_than.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            parse_num(match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing number.")}),
            parse_num(match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing number.")})
        );

        (n1 < n2).to_string()
    });

    /*- Replace equals -*/
    let input = equals.replace_all(&input, |caps: &Captures| {
        let (n1, n2) = (
            match caps.get(1) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing string.")},
            match caps.get(2) { Some(s) => s.as_str().to_string(), None => return String::from("Error parsing string.")}
        );

        (n1 == n2).to_string()
    });

    /*- Return -*/
    input.parse::<bool>()
}
