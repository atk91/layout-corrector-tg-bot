extern crate reqwest;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::iter::FromIterator;

fn fix_layout(src: &str) -> String {
    let latin_chars = "qwertyuiop[]asdfghjkl;'zxcvbnm,./?`&";
    let cyrillic_chars = "йцукенгшщзхъфывапролджэячсмитьбю.,ё?";
    assert!(latin_chars.chars().count() == cyrillic_chars.chars().count());
    let chars_map: HashMap<char, char> =
        HashMap::from_iter(latin_chars.chars().zip(cyrillic_chars.chars()));
    String::from_iter(
        src.chars()
            .map(|x| chars_map.get(&x).map(|y| *y).unwrap_or(x)),
    )
}

enum WordLanguage {
    Russian,
    NotRussian,
}

fn get_russians_ratio(src: &str, words: &HashSet<String>) -> f32 {
    if src
        .find(|x| "йцукенгшщзхъфывапролджэячсмитьбю".contains(x))
        .is_some()
    {
        return -1.0;
    }
    let punctuation = "!\"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~";
    let (total_count, russian_count) = src
        .split_whitespace()
        .map(|s| {
            if words.contains(&String::from_iter(
                fix_layout(s).chars().filter(|c| !punctuation.contains(*c)),
            )) {
                WordLanguage::Russian
            } else {
                WordLanguage::NotRussian
            }
        })
        .fold((0, 0), |acc, elem| {
            (
                acc.0 + 1,
                acc.1
                    + match elem {
                        WordLanguage::Russian => 1,
                        WordLanguage::NotRussian => 0,
                    },
            )
        });
    if total_count != 0 {
        println!("ratio is {}", russian_count as f32 / total_count as f32);
        russian_count as f32 / total_count as f32
    } else {
        println!("ratio is 0.0");
        0.0
    }
}

#[derive(Serialize, Deserialize)]
struct Chat {
    first_name: Option<String>,
    id: i64,
    last_name: Option<String>,
    username: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    chat: Chat,
    date: i64,
    text: Option<String>,
    message_id: i64,
}

#[derive(Serialize, Deserialize)]
struct Update {
    message: Option<Message>,
    update_id: i64,
}

fn log_updates(updates: &Vec<Update>) {
    for u in updates {
        println!(
            "UPDATES_LOG: update_id={} chat_id={}: text={}",
            u.update_id,
            u.message.as_ref().map(|x| x.chat.id).unwrap_or(-1),
            if u.message.is_some() && u.message.as_ref().unwrap().text.is_some() {
                &*u.message.as_ref().unwrap().text.as_ref().unwrap()
            } else {
                "NONE"
            }
        );
    }
}

fn reply_to_message(chat_id: &str, message_id: &str, text: &str, token: &str) {
    reqwest::Client::new()
        .post(&*format!(
            "https://api.telegram.org/bot{}/sendMessage",
            token
        ))
        .form(&[
            ("chat_id", chat_id),
            ("reply_to_message_id", message_id),
            ("text", text),
        ])
        .send()
        .unwrap();
}

fn get_available_updates(
    token: &str,
    last_confirmed: &i64,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let updates_url = &*format!("https://api.telegram.org/bot{}/getUpdates", token);
    let updates = reqwest::Client::new()
        .post(updates_url)
        .form(&[("offset", *last_confirmed + 1)])
        .send()?
        .json()?;
    Ok(updates)
}

fn get_and_process_updates(
    words: &HashSet<String>,
    last_confirmed: &mut i64,
    token: &str,
) -> Result<(), Box<dyn Error>> {
    let updates = get_available_updates(&*token, &last_confirmed)?;
    match &updates["ok"].as_bool() {
        Some(true) => Ok(true),
        Some(false) => Err("Ok from telegram api is false"),
        None => Err("No ok field in response from telegram api"),
    }?;
    let updates: Vec<Update> = serde_json::from_value(updates["result"].clone())?;
    log_updates(&updates);
    let last_update_id = updates.last().map(|x| x.update_id);
    if last_update_id.is_some() {
        *last_confirmed = last_update_id.unwrap();
    }
    for u in updates {
        let message_id = u.message.as_ref().map(|x| x.message_id);
        let chat_id = u.message.as_ref().map(|x| x.chat.id);
        let text = u.message.and_then(|x| x.text.map(|y| y.to_lowercase()));
        let ratio = text.as_ref().map(|x| get_russians_ratio(&*x, &words));
        let threshold = 0.4;
        if ratio.unwrap_or(-1.0) > threshold {
            let translated = fix_layout(&*text.unwrap().to_lowercase());
            let message_id = message_id.unwrap();
            let chat_id = chat_id.unwrap();
            reply_to_message(
                &*chat_id.to_string(),
                &*message_id.to_string(),
                &*translated,
                token,
            );
        }
    }
    Ok(())
}

fn build_words(filename: &str) -> Result<HashSet<String>, Box<dyn Error>> {
    let mut file = File::open(filename)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(HashSet::from_iter(buf.split("\n").map(|s| String::from(s))))
}

fn read_token(filename: &str) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(filename)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(buf)
}

fn main() -> Result<(), Box<dyn Error>> {
    let (words_filename, token_filename) = (
        std::env::args().nth(1).unwrap(),
        std::env::args().nth(2).unwrap(),
    );
    let words = build_words(&*words_filename)?;
    let token = read_token(&*token_filename)?;
    println!("words array built!");
    let mut last_confirmed = 0;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        match get_and_process_updates(&words, &mut last_confirmed, &*token) {
            Ok(_) => (),
            Err(e) => println!("Processing updates failed: {}", e),
        }
    }
}
