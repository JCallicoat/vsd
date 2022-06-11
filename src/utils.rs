use std::io::Write;

use anyhow::Result;

pub fn format_bytes(bytesval: usize) -> (String, String, String) {
    let mut val = bytesval as f32;

    for unit in ["bytes", "KB", "MB", "GB", "TB"] {
        if val < 1024.0 {
            return (
                format!("{:.2}", val),
                unit.to_owned(),
                format!("{:.2} {}", val, unit),
            );
        }

        val /= 1024.0;
    }

    return (
        format!("{:.2}", bytesval),
        "".to_owned(),
        format!("{:.2}", bytesval),
    );
}

pub fn find_hls_dash_links(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"(https|ftp|http)://([\w_-]+(?:(?:\.[\w_-]+)+))([\w.,@?^=%&:/~+#-]*[\w@?^=%&/~+#-]\.(m3u8|m3u|mpd))").unwrap();
    let links = re
        .captures_iter(&text)
        .map(|caps| caps.get(0).unwrap().as_str().to_string())
        .collect::<Vec<String>>();

    let mut unique_links = vec![];
    for link in links {
        if !unique_links.contains(&link) {
            unique_links.push(link);
        }
    }
    unique_links
}

pub fn select(prompt: String, choices: &Vec<String>, raw: bool) -> Result<usize> {
    if raw {
        println!("{}", prompt);

        for choice in choices {
            println!("{}", choice);
        }

        print!("{} (1, 2, etc.): ", prompt);
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        return Ok(input.trim().parse::<usize>()? - 1);
    }

    Ok(requestty::prompt_one(
        requestty::Question::select("theme")
            .message(prompt)
            .choices(choices)
            .build(),
    )?
    .as_list_item()
    .unwrap()
    .index)
}

pub fn find_ffmpeg_with_path() -> Option<String> {
    Some(
        std::env::var("PATH")
            .ok()?
            .split(if cfg!(target_os = "windows") {
                ';'
            } else {
                ':'
            })
            .find(|s| {
                std::path::Path::new(s)
                    .join(if cfg!(target_os = "windows") {
                        "ffmpeg.exe"
                    } else {
                        "ffmpeg"
                    })
                    .exists()
            })?
            .to_owned(),
    )
}

// pub fn join_path(pth1: &str, pth2: &str) -> String {
//     Path::new(pth1).join(pth2).to_str().unwrap().to_owned()
// }

pub fn replace_ext(pth: &str, ext: &str) -> String {
    let mut tpth = std::path::PathBuf::from(pth);
    tpth.set_extension(ext);
    tpth.to_str().unwrap().to_owned()
}
