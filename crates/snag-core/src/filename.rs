use serde::Deserialize;
use std::fmt::Write;

pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct FilenameTemplate(Vec<Part>);

impl Default for FilenameTemplate {
    fn default() -> Self {
        Self::try_from("screenshot-%Y%m%d-%H%M%S".to_owned()).expect("valid default filename")
    }
}

impl TryFrom<String> for FilenameTemplate {
    type Error = String;

    fn try_from(template: String) -> Result<Self, Self::Error> {
        if template.is_empty() || template.contains('/') {
            return Err("save.filename must be a nonempty filename without slashes".into());
        }
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut chars = template.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                literal.push(ch);
                continue;
            }
            let part = match chars.next() {
                Some('%') => {
                    literal.push('%');
                    continue;
                }
                Some('Y') => Part::Year,
                Some('m') => Part::Month,
                Some('d') => Part::Day,
                Some('H') => Part::Hour,
                Some('M') => Part::Minute,
                Some('S') => Part::Second,
                Some(token) => return Err(format!("unknown save.filename token %{token}")),
                None => return Err("trailing % in save.filename".into()),
            };
            if !literal.is_empty() {
                parts.push(Part::Text(std::mem::take(&mut literal)));
            }
            parts.push(part);
        }
        if !literal.is_empty() {
            parts.push(Part::Text(literal));
        }
        Ok(Self(parts))
    }
}

impl FilenameTemplate {
    pub fn render(&self, time: &LocalTime) -> String {
        let mut name = String::new();
        for part in &self.0 {
            match part {
                Part::Text(text) => name.push_str(text),
                Part::Year => write!(name, "{:04}", time.year).unwrap(),
                Part::Month => write!(name, "{:02}", time.month).unwrap(),
                Part::Day => write!(name, "{:02}", time.day).unwrap(),
                Part::Hour => write!(name, "{:02}", time.hour).unwrap(),
                Part::Minute => write!(name, "{:02}", time.minute).unwrap(),
                Part::Second => write!(name, "{:02}", time.second).unwrap(),
            }
        }
        name.push_str(".png");
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_local_fields_literals_and_percent() {
        let time = LocalTime {
            year: 2026,
            month: 3,
            day: 9,
            hour: 4,
            minute: 8,
            second: 1,
        };
        assert_eq!(
            FilenameTemplate::default().render(&time),
            "screenshot-20260309-040801.png"
        );
        let template = FilenameTemplate::try_from("é%%-%S-%S".to_owned()).unwrap();
        assert_eq!(template.render(&time), "é%-01-01.png");
    }

    #[test]
    fn rejects_invalid_names_and_tokens() {
        for invalid in ["", "bad/name", "%y", "%"] {
            assert!(
                FilenameTemplate::try_from(invalid.to_owned()).is_err(),
                "{invalid}"
            );
        }
    }
}
