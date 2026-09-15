//! Startup options and the shared planet picker used by both launchers.

use bevy::prelude::*;
use std::io::{self, IsTerminal, Write};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlanetKind {
    #[default]
    Field,
    Hex,
}

impl PlanetKind {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "field" => Ok(Self::Field),
            "hex" => Ok(Self::Hex),
            _ => Err(format!("unknown planet type '{value}'; use --list-planets")),
        }
    }
}

fn choices() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    include_str!("../../../assets/config/planets.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .filter_map(|line| {
            let mut columns = line.split('\t');
            Some((columns.next()?, columns.next()?, columns.next()?))
        })
}

#[derive(Resource, Clone, Debug)]
pub struct Args {
    pub planet: PlanetKind,
    pub sub: usize,
    pub wire: bool,
    pub fly: bool,
    pub eye: Option<Vec3>,
    pub look: Option<Vec3>,
    pub shot: Option<String>,
    pub frames: u32,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            planet: PlanetKind::Field,
            sub: 4,
            wire: false,
            fly: false,
            eye: None,
            look: None,
            shot: None,
            frames: 30,
        }
    }
}

fn vector(value: &str) -> Result<Vec3, String> {
    let values: Result<Vec<f32>, _> = value.split(',').map(|s| s.trim().parse()).collect();
    match values {
        Ok(v) if v.len() == 3 && v.iter().all(|n| n.is_finite()) => Ok(Vec3::new(v[0], v[1], v[2])),
        _ => Err(format!("expected three finite coordinates, got '{value}'")),
    }
}

fn parse(values: impl IntoIterator<Item = String>) -> Result<(Args, bool), String> {
    let mut args = Args::default();
    let mut selected = false;
    let mut it = values.into_iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--wire" => args.wire = true,
            "--fly" => args.fly = true,
            "--planet" | "--sub" | "--eye" | "--look" | "--shot" | "--frames" => {
                let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--planet" => {
                        args.planet = PlanetKind::parse(&value)?;
                        selected = true;
                    }
                    "--sub" => {
                        args.sub = value
                            .parse::<usize>()
                            .map_err(|_| "--sub needs an integer")?
                            .clamp(1, 8)
                    }
                    "--eye" => args.eye = Some(vector(&value)?),
                    "--look" => args.look = Some(vector(&value)?),
                    "--shot" => args.shot = Some(value),
                    "--frames" => {
                        args.frames = value
                            .parse::<u32>()
                            .map_err(|_| "--frames needs an integer")?
                            .clamp(1, u32::MAX - 12)
                    }
                    _ => unreachable!(),
                }
            }
            _ => return Err(format!("unknown argument '{flag}'; use --help")),
        }
    }
    Ok((args, selected))
}

fn list_planets() {
    for (index, (id, name, description)) in choices().enumerate() {
        println!("  {}. {name} ({id})\n     {description}", index + 1);
    }
}

fn pick_planet() -> Result<PlanetKind, String> {
    println!("\nChoose a planet to create:");
    list_planets();
    loop {
        print!("Planet [1]: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        let value = answer.trim();
        let id = if value.is_empty() {
            "field"
        } else if let Ok(index) = value.parse::<usize>() {
            choices()
                .nth(index.saturating_sub(1))
                .filter(|_| index > 0)
                .map(|c| c.0)
                .unwrap_or(value)
        } else {
            value
        };
        match PlanetKind::parse(id) {
            Ok(kind) => return Ok(kind),
            Err(error) => println!("{error}"),
        }
    }
}

pub fn startup() -> Result<Option<Args>, String> {
    let values: Vec<_> = std::env::args().skip(1).collect();
    if values
        .iter()
        .any(|s| s == "--help" || s == "-h" || s == "--list-planets")
    {
        println!(
            "freeport_app [--planet TYPE] [--fly] [--wire] [--sub N]\n\
                  [--eye x,y,z] [--look x,y,z] [--shot out.png] [--frames N]\n"
        );
        list_planets();
        return Ok(None);
    }
    let (mut args, selected) = parse(values)?;
    if !selected && args.shot.is_none() && io::stdin().is_terminal() && io::stdout().is_terminal() {
        args.planet = pick_planet()?;
    }
    Ok(Some(args))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Result<(Args, bool), String> {
        parse(values.iter().map(|s| s.to_string()))
    }

    #[test]
    fn planet_selection_preserves_render_options() {
        for (id, _, _) in choices() {
            let (args, selected) =
                arguments(&["--planet", id, "--fly", "--shot", "a.png"]).unwrap();
            assert_eq!(args.planet, PlanetKind::parse(id).unwrap());
            assert!(selected && args.fly);
            assert_eq!(args.shot.as_deref(), Some("a.png"));
        }
        assert_eq!(arguments(&[]).unwrap().0.planet, PlanetKind::Field);
    }

    #[test]
    fn invalid_planets_and_camera_coordinates_report_errors() {
        for values in [
            vec!["--planet"],
            vec!["--planet", "hexx"],
            vec!["--eye", "NaN,1,2"],
            vec!["--look", "1,no,2,3"],
        ] {
            assert!(arguments(&values).is_err());
        }
    }
}
