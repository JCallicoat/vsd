use crate::commands::Quality;
use anyhow::{bail, Result};
use kdam::term::Colorizer;
use requestty::prompt::style::Stylize;
use reqwest::Url;
use serde::Serialize;
use std::{fmt::Display, io::Write, path::PathBuf};

#[derive(Clone, Default, PartialEq, Serialize)]
pub(crate) enum MediaType {
    Audio,
    Subtitles,
    #[default]
    Undefined,
    Video,
}

impl Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Audio => "audio",
                Self::Subtitles => "subtitles",
                Self::Undefined => "undefined",
                Self::Video => "video",
            }
        )
    }
}

#[derive(Default, Serialize)]
pub(crate) enum PlaylistType {
    Dash,
    #[default]
    Hls,
}

#[derive(Serialize)]
pub(crate) struct ByteRange {
    pub(crate) length: u64,
    pub(crate) offset: Option<u64>,
}

#[derive(Serialize)]
pub(crate) struct Map {
    pub(crate) uri: String,
    pub(crate) byte_range: Option<ByteRange>,
}

#[derive(Clone, Serialize, PartialEq)]
pub(crate) enum KeyMethod {
    Aes128,
    Cenc,
    None,
    Other(String),
    SampleAes,
}

#[derive(Serialize)]
pub(crate) struct Key {
    pub(crate) default_kid: Option<String>,
    pub(crate) iv: Option<String>,
    pub(crate) key_format: Option<String>,
    pub(crate) method: KeyMethod,
    pub(crate) uri: String,
}

#[derive(Default, Serialize)]
pub(crate) struct Segment {
    pub(crate) byte_range: Option<ByteRange>,
    // TODO - Support #EXT-X-DISCOUNTINUITY tag
    // pub(crate) discountinuity: bool,
    pub(crate) duration: f32,
    pub(crate) key: Option<Key>,
    pub(crate) map: Option<Map>,
    pub(crate) uri: String,
}

impl Segment {
    pub(crate) fn seg_url(&self, baseurl: &Url) -> Result<Url> {
        if self.uri.starts_with("http") || self.uri.starts_with("ftp") {
            Ok(self.uri.parse::<Url>()?)
        } else {
            Ok(baseurl.join(&self.uri)?)
        }
    }

    pub(crate) fn map_url(&self, baseurl: &Url) -> Result<Option<Url>> {
        if let Some(map) = &self.map {
            if self.uri.starts_with("http") || self.uri.starts_with("ftp") {
                return Ok(Some(map.uri.parse::<Url>()?));
            } else {
                return Ok(Some(baseurl.join(&map.uri)?));
            }
        }

        Ok(None)
    }

    pub(crate) fn key_url(&self, baseurl: &Url) -> Result<Option<Url>> {
        if let Some(key) = &self.key {
            if self.uri.starts_with("http") || self.uri.starts_with("ftp") {
                return Ok(Some(key.uri.parse::<Url>()?));
            } else {
                return Ok(Some(baseurl.join(&key.uri)?));
            }
        }

        Ok(None)
    }

    pub(crate) fn seg_range(&self, previous_byterange_end: u64) -> Option<(String, u64)> {
        if let Some(byte_range) = &self.byte_range {
            let offset = byte_range.offset.unwrap_or(0);

            let (start, end) = if offset == 0 {
                (
                    previous_byterange_end,
                    previous_byterange_end + byte_range.length - 1,
                )
            } else {
                (byte_range.length, byte_range.length + offset - 1)
            };

            Some((format!("bytes={}-{}", start, end), end))
        } else {
            None
        }
    }

    pub(crate) fn map_range(&self, previous_byterange_end: u64) -> Option<(String, u64)> {
        if let Some(map) = &self.map {
            if let Some(byte_range) = &map.byte_range {
                let offset = byte_range.offset.unwrap_or(0);

                let (start, end) = if offset == 0 {
                    (
                        previous_byterange_end,
                        previous_byterange_end + byte_range.length - 1,
                    )
                } else {
                    (byte_range.length, byte_range.length + offset - 1)
                };

                return Some((format!("bytes={}-{}", start, end), end));
            }
        }

        None
    }
}

#[derive(Default, Serialize)]
pub(crate) struct MediaPlaylist {
    pub(crate) bandwidth: Option<u64>,
    pub(crate) channels: Option<f32>,
    pub(crate) codecs: Option<String>,
    pub(crate) extension: Option<String>,
    pub(crate) frame_rate: Option<f32>,
    pub(crate) i_frame: bool,
    pub(crate) language: Option<String>,
    pub(crate) live: bool,
    pub(crate) media_type: MediaType,
    pub(crate) playlist_type: PlaylistType,
    pub(crate) resolution: Option<(u64, u64)>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) uri: String,
}

impl MediaPlaylist {
    fn has_resolution(&self, w: u16, h: u16) -> bool {
        if let Some((video_w, video_h)) = self.resolution {
            w as u64 == video_w && h as u64 == video_h
        } else {
            false
        }
    }

    fn display_video_stream(&self) -> String {
        let resolution = if let Some((w, h)) = self.resolution {
            match (w, h) {
                (256, 144) => "144p".to_owned(),
                (426, 240) => "240p".to_owned(),
                (640, 360) => "360p".to_owned(),
                (854, 480) => "480p".to_owned(),
                (1280, 720) => "720p".to_owned(),
                (1920, 1080) => "1080p".to_owned(),
                (2048, 1080) => "2K".to_owned(),
                (2560, 1440) => "1440p".to_owned(),
                (3840, 2160) => "4K".to_owned(),
                (7680, 4320) => "8K".to_owned(),
                (w, h) => format!("{}x{}", w, h),
            }
        } else {
            "?".to_owned()
        };

        let bandwidth = if let Some(bandwidth) = self.bandwidth {
            crate::utils::format_bytes(bandwidth as usize, 2)
        } else {
            ("?".to_owned(), "?".to_owned(), "?".to_owned())
        };

        let mut extra = format!(
            "(codecs: {}",
            self.codecs.as_ref().unwrap_or(&"?".to_owned())
        );

        if let Some(frame_rate) = self.frame_rate {
            extra += &format!(", frame_rate: {}", frame_rate);
        }

        if self.i_frame {
            extra += ", iframe";
        }

        if self.live {
            extra += ", live";
        }

        extra += ")";

        format!(
            "{:9} {:>7} {}/s {}",
            resolution, bandwidth.0, bandwidth.1, extra
        )
    }

    fn display_audio_stream(&self) -> String {
        let mut extra = format!(
            "language: {}",
            self.language.as_ref().unwrap_or(&"?".to_owned())
        );

        if let Some(codecs) = &self.codecs {
            extra += &format!(", codecs: {}", codecs);
        }

        if let Some(bandwidth) = self.bandwidth {
            extra += &format!(
                ", bandwidth: {}/s",
                crate::utils::format_bytes(bandwidth as usize, 2).2
            );
        }

        if let Some(channels) = self.channels {
            extra += &format!(", channels: {}", channels);
        }

        if self.live {
            extra += ", live";
        }

        extra
    }

    pub(crate) fn display_subtitle_stream(&self) -> String {
        let mut extra = format!(
            "language: {}",
            self.language.as_ref().unwrap_or(&"?".to_owned())
        );

        if let Some(codecs) = &self.codecs {
            extra += &format!(", codecs: {}", codecs);
        }

        extra
    }

    pub(crate) fn display_stream(&self) -> String {
        match self.media_type {
            MediaType::Audio => self.display_audio_stream(),
            MediaType::Subtitles => self.display_subtitle_stream(),
            MediaType::Undefined => "".to_owned(),
            MediaType::Video => self.display_video_stream(),
        }
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
    }

    pub(crate) fn url(&self, baseurl: &Url) -> Result<Url> {
        // self.uri.starts_with("dash://")
        if self.uri.starts_with("http") || self.uri.starts_with("ftp") {
            Ok(self.uri.parse::<Url>()?)
        } else {
            Ok(baseurl.join(&self.uri)?)
        }
    }

    pub(crate) fn is_hls(&self) -> bool {
        match &self.playlist_type {
            PlaylistType::Hls => true,
            _ => false,
        }
    }

    // pub(crate) fn is_dash(&self) -> bool {
    //     match &self.playlist_type {
    //         PlaylistType::Dash => true,
    //         _ => false,
    //     }
    // }

    pub(crate) fn extension(&self) -> String {
        if let Some(ext) = &self.extension {
            return ext.to_owned();
        }

        let mut ext = match &self.playlist_type {
            PlaylistType::Hls => "ts",
            PlaylistType::Dash => "m4s",
        };

        if let Some(segment) = self.segments.get(0) {
            if let Some(init) = &segment.map {
                if init.uri.ends_with(".mp4") {
                    ext = "mp4";
                }
            }

            if segment.uri.ends_with(".mp4") {
                ext = "mp4";
            }
        }

        ext.to_owned()
    }

    pub(crate) fn file_path(&self, directory: &Option<PathBuf>, ext: &str) -> PathBuf {
        let filename = PathBuf::from(
            self.uri
                .split('?')
                .next()
                .unwrap()
                .split('/')
                .last()
                .unwrap_or("undefined")
                .chars()
                .map(|x| match x {
                    '<' | '>' | ':' | '\"' | '\\' | '|' | '?' => '_',
                    _ => x,
                })
                .collect::<String>(),
        )
        .with_extension("");

        let suffix = match &self.media_type {
            MediaType::Audio => "audio",
            MediaType::Subtitles => "subtitles",
            MediaType::Undefined => "undefined",
            MediaType::Video => "video",
        };

        let mut path = PathBuf::from(format!(
            "vsd_{}_{}.{}",
            suffix,
            filename.to_string_lossy(),
            ext
        ));

        if let Some(directory) = directory {
            path = directory.join(path);
        }

        if path.exists() {
            for i in 1.. {
                path.set_file_name(format!(
                    "vsd_{}_{}_({}).{}",
                    suffix,
                    filename.to_string_lossy(),
                    i,
                    ext
                ));

                if !path.exists() {
                    return path;
                }
            }
        }

        path
    }

    // pub(crate) fn is_encrypted(&self) -> bool {
    //     match &self.playlist_type {
    //         PlaylistType::Dash => {
    //             if let Some(segment) = self.segments.get(0) {
    //                 return segment.key.is_some();
    //             }
    //         }
    //         PlaylistType::Hls => {
    //             for segment in &self.segments {
    //                 if segment.key.is_some() {
    //                     return true;
    //                 }
    //             }
    //         }
    //     }

    //     false
    // }

    pub(crate) fn default_kid(&self) -> Option<String> {
        if let Some(segment) = self.segments.get(0) {
            if let Some(Key {
                default_kid: Some(x),
                ..
            }) = &segment.key
            {
                return Some(x.replace('-', "").to_lowercase());
            }
        }

        None
    }
}

#[derive(Serialize)]
pub(crate) struct MasterPlaylist {
    pub(crate) playlist_type: PlaylistType,
    pub(crate) uri: String,
    pub(crate) streams: Vec<MediaPlaylist>,
}

impl MasterPlaylist {
    // pub(crate) fn url(&self, baseurl: &str) -> Result<Url> {
    //     if self.uri.starts_with("http") || self.uri.starts_with("ftp") {
    //         Ok(self.uri.parse::<Url>()?)
    //     } else {
    //         Ok(baseurl.parse::<Url>()?.join(&self.uri)?)
    //     }
    // }

    // pub(crate) fn is_hls(&self) -> bool {
    //     match self.playlist_type {
    //         PlaylistType::Hls => true,
    //         _ => false,
    //     }
    // }

    // pub(crate) fn is_dash(&self) -> bool {
    //     match self.playlist_type {
    //         PlaylistType::Dash => true,
    //         _ => false,
    //     }
    // }

    pub(crate) fn sort_streams(
        mut self,
        prefer_audio_lang: Option<String>,
        prefer_subs_lang: Option<String>,
    ) -> Self {
        let prefer_audio_lang = prefer_audio_lang.map(|x| x.to_lowercase());
        let prefer_subs_lang = prefer_subs_lang.map(|x| x.to_lowercase());

        let mut video_streams = vec![];
        let mut audio_streams = vec![];
        let mut subtitle_streams = vec![];
        let mut undefined_streams = vec![];

        for stream in self.streams {
            match stream.media_type {
                MediaType::Audio => {
                    let mut language_factor = 0;

                    if let Some(playlist_lang) = &stream.language.as_ref().map(|x| x.to_lowercase())
                    {
                        if let Some(prefer_lang) = &prefer_audio_lang {
                            if playlist_lang == prefer_lang {
                                language_factor = 2;
                            } else if playlist_lang.get(0..2) == prefer_lang.get(0..2) {
                                language_factor = 1;
                            }
                        }
                    }

                    let channels = stream.channels.unwrap_or(0.0);
                    let bandwidth = stream.bandwidth.unwrap_or(0);

                    audio_streams.push((stream, language_factor, channels, bandwidth));
                }
                MediaType::Subtitles => {
                    let mut language_factor = 0;

                    if let Some(playlist_lang) = &stream.language.as_ref().map(|x| x.to_lowercase())
                    {
                        if let Some(prefer_lang) = &prefer_subs_lang {
                            if playlist_lang == prefer_lang {
                                language_factor = 2;
                            } else if playlist_lang.get(0..2) == prefer_lang.get(0..2) {
                                language_factor = 1;
                            }
                        }
                    }

                    subtitle_streams.push((stream, language_factor));
                }
                MediaType::Undefined => undefined_streams.push(stream),
                MediaType::Video => {
                    let pixels = if let Some((w, h)) = &stream.resolution {
                        w * h
                    } else {
                        0
                    };

                    let bandwidth = stream.bandwidth.unwrap_or(0);

                    video_streams.push((stream, pixels, bandwidth));
                }
            }
        }

        video_streams.sort_by(|x, y| y.2.cmp(&x.2));
        video_streams.sort_by(|x, y| y.1.cmp(&x.1));
        audio_streams.sort_by(|x, y| y.3.cmp(&x.3));
        audio_streams.sort_by(|x, y| y.2.total_cmp(&x.2));
        audio_streams.sort_by(|x, y| y.1.cmp(&x.1));
        subtitle_streams.sort_by(|x, y| y.1.cmp(&x.1));

        self.streams = video_streams
            .into_iter()
            .map(|x| x.0)
            .chain(audio_streams.into_iter().map(|x| x.0))
            .chain(subtitle_streams.into_iter().map(|x| x.0))
            .chain(undefined_streams.into_iter())
            .collect::<Vec<_>>();

        self
    }

    pub(crate) fn select_streams(
        self,
        quality: Quality,
        skip_prompts: bool,
        raw_prompts: bool,
    ) -> Result<(Vec<MediaPlaylist>, Vec<MediaPlaylist>)> {
        let mut video_streams = self
            .streams
            .iter()
            .filter(|x| x.media_type == MediaType::Video)
            .enumerate();

        let default_video_stream_index = match &quality {
            Quality::Lowest => Some(video_streams.count() - 1),
            Quality::Highest => Some(0),
            Quality::Resolution(w, h) => video_streams
                .find(|x| x.1.has_resolution(*w, *h))
                .map(|y| y.0),
            Quality::Youtube144p => video_streams
                .find(|x| x.1.has_resolution(256, 144))
                .map(|y| y.0),
            Quality::Youtube240p => video_streams
                .find(|x| x.1.has_resolution(426, 240))
                .map(|y| y.0),
            Quality::Youtube360p => video_streams
                .find(|x| x.1.has_resolution(640, 360))
                .map(|y| y.0),
            Quality::Youtube480p => video_streams
                .find(|x| x.1.has_resolution(854, 480))
                .map(|y| y.0),
            Quality::Youtube720p => video_streams
                .find(|x| x.1.has_resolution(1280, 720))
                .map(|y| y.0),
            Quality::Youtube1080p => video_streams
                .find(|x| x.1.has_resolution(1920, 1080))
                .map(|y| y.0),
            Quality::Youtube2k => video_streams
                .find(|x| x.1.has_resolution(2048, 1080))
                .map(|y| y.0),
            Quality::Youtube1440p => video_streams
                .find(|x| x.1.has_resolution(2560, 1440))
                .map(|y| y.0),
            Quality::Youtube4k => video_streams
                .find(|x| x.1.has_resolution(3840, 2160))
                .map(|y| y.0),
            Quality::Youtube8k => video_streams
                .find(|x| x.1.has_resolution(7680, 4320))
                .map(|y| y.0),
        };

        if let Some(default_video_stream_index) = default_video_stream_index {
            let mut video_streams = vec![];
            let mut audio_streams = vec![];
            let mut subtitle_streams = vec![];
            let mut undefined_streams = vec![];

            for stream in self.streams {
                match stream.media_type {
                    MediaType::Audio => audio_streams.push(stream),
                    MediaType::Subtitles => subtitle_streams.push(stream),
                    MediaType::Undefined => undefined_streams.push(stream),
                    MediaType::Video => video_streams.push(stream),
                }
            }

            let mut choices_with_default = vec![];
            let mut choices_with_default_ranges: [std::ops::Range<usize>; 4] =
                [(0..0), (0..0), (0..0), (0..0)];

            choices_with_default.push(requestty::Separator(
                "─────── Video Streams ────────".to_owned(),
            ));
            choices_with_default.extend(video_streams.iter().enumerate().map(|(i, x)| {
                requestty::Choice((x.display_video_stream(), i == default_video_stream_index))
            }));
            choices_with_default_ranges[0] = 1..choices_with_default.len();
            choices_with_default.push(requestty::Separator(
                "─────── Audio Streams ────────".to_owned(),
            ));
            choices_with_default.extend(
                audio_streams
                    .iter()
                    .enumerate()
                    .map(|(i, x)| requestty::Choice((x.display_audio_stream(), i == 0))),
            );

            if skip_prompts || raw_prompts {
                choices_with_default_ranges[1] =
                    choices_with_default_ranges[0].end..(choices_with_default.len() - 1);
            } else {
                choices_with_default_ranges[1] =
                    (choices_with_default_ranges[0].end + 1)..choices_with_default.len();
            }

            choices_with_default.push(requestty::Separator(
                "────── Subtitle Streams ──────".to_owned(),
            ));
            choices_with_default.extend(
                subtitle_streams
                    .iter()
                    .enumerate()
                    .map(|(i, x)| requestty::Choice((x.display_subtitle_stream(), i == 0))),
            );

            if skip_prompts || raw_prompts {
                choices_with_default_ranges[2] =
                    choices_with_default_ranges[1].end..(choices_with_default.len() - 2);
            } else {
                choices_with_default_ranges[2] =
                    (choices_with_default_ranges[1].end + 1)..choices_with_default.len();
            }

            // println!("{:?}", choices_with_default_ranges);

            if skip_prompts || raw_prompts {
                println!("Select streams to download:");
                let mut selected_choices_index = vec![];
                let mut index = 1;

                for choice in choices_with_default {
                    if let requestty::Separator(seperator) = choice {
                        println!("{}", seperator.replace('─', "-"));
                    } else {
                        let (message, selected) = choice.unwrap_choice();

                        if selected {
                            selected_choices_index.push(index);
                        }

                        println!(
                            "{:2}) [{}] {}",
                            index,
                            if selected { 'x' } else { ' ' },
                            message
                        );
                        index += 1;
                    }
                }

                println!("------------------------------");

                if raw_prompts && !skip_prompts {
                    print!(
                        "Press enter to proceed with defaults.\n\
                        Or select streams to download (1, 2, etc.): "
                    );
                    std::io::stdout().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;

                    println!("------------------------------");

                    let input = input.trim();

                    if input != "" {
                        selected_choices_index = input
                            .split(',')
                            .filter_map(|x| x.trim().parse::<usize>().ok())
                            .collect::<Vec<usize>>();
                    }
                }

                let mut selected_streams = vec![];
                let mut selected_subtitle_streams = vec![];
                let mut video_streams_offset = 1;
                let mut audio_streams_offset = video_streams_offset + video_streams.len();
                let mut subtitle_streams_offset = audio_streams_offset + audio_streams.len();

                for i in selected_choices_index {
                    if choices_with_default_ranges[0].contains(&i) {
                        let stream = video_streams.remove(i - video_streams_offset);
                        println!(
                            "   {} {}",
                            "Selected".colorize("bold green"),
                            stream.display_stream()
                        );
                        selected_streams.push(stream);
                        video_streams_offset += 1;
                    } else if choices_with_default_ranges[1].contains(&i) {
                        let stream = audio_streams.remove(i - audio_streams_offset);
                        println!(
                            "   {} {}",
                            "Selected".colorize("bold green"),
                            stream.display_stream()
                        );
                        selected_streams.push(stream);
                        audio_streams_offset += 1;
                    } else if choices_with_default_ranges[2].contains(&i) {
                        let stream = subtitle_streams.remove(i - subtitle_streams_offset);
                        println!(
                            "   {} {}",
                            "Selected".colorize("bold green"),
                            stream.display_stream()
                        );
                        selected_subtitle_streams.push(stream);
                        subtitle_streams_offset += 1;
                    }
                }

                Ok((selected_streams, selected_subtitle_streams))
            } else {
                let question = requestty::Question::multi_select("streams")
                    .should_loop(false)
                    .message("Select streams to download")
                    .choices_with_default(choices_with_default)
                    .transform(|choices, _, backend| {
                        backend.write_styled(
                            &choices
                                .iter()
                                .map(|x| x.text.split_whitespace().collect::<Vec<_>>().join(" "))
                                .collect::<Vec<_>>()
                                .join(" | ")
                                .cyan(),
                        )
                    })
                    .build();

                let answer = requestty::prompt_one(question)?;

                let mut selected_streams = vec![];
                let mut selected_subtitle_streams = vec![];
                let mut video_streams_offset = 1;
                let mut audio_streams_offset = video_streams_offset + video_streams.len() + 1;
                let mut subtitle_streams_offset = audio_streams_offset + audio_streams.len() + 1;

                for selected_item in answer.as_list_items().unwrap() {
                    if choices_with_default_ranges[0].contains(&selected_item.index) {
                        selected_streams
                            .push(video_streams.remove(selected_item.index - video_streams_offset));
                        video_streams_offset += 1;
                    } else if choices_with_default_ranges[1].contains(&selected_item.index) {
                        selected_streams
                            .push(audio_streams.remove(selected_item.index - audio_streams_offset));
                        audio_streams_offset += 1;
                    } else if choices_with_default_ranges[2].contains(&selected_item.index) {
                        selected_subtitle_streams.push(
                            subtitle_streams.remove(selected_item.index - subtitle_streams_offset),
                        );
                        subtitle_streams_offset += 1;
                    }
                }

                Ok((selected_streams, selected_subtitle_streams))
            }
        } else {
            // TODO - Add better message
            // Selected variant stream of quality {} ({} {}/s).
            bail!("playlist doesn't contain {:?} quality stream", quality)
        }
    }
}