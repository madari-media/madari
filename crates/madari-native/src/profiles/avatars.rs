//! The curated profile artwork catalog shared by native clients.
use serde::Serialize;

pub const BASE_URL: &str = "https://downloads.madari.media/profile_compressed/";
pub const FILES: &[&str] = &[
    "Airplane.webp",
    "Android.webp",
    "Apple But Tech.webp",
    "Apple.webp",
    "Baby 1.webp",
    "Baby 2.webp",
    "Baby 3.webp",
    "Banana.webp",
    "Black Cat.webp",
    "Building.webp",
    "Child 1.webp",
    "Child 2.webp",
    "Child 3.webp",
    "Computer Apple.webp",
    "Controller.webp",
    "Corgy Dog.webp",
    "Dragon.webp",
    "Duck.webp",
    "Easter Egg.webp",
    "Fox.webp",
    "Ginger Cat.webp",
    "Girl 1.webp",
    "Girl 2.webp",
    "Girl 3.webp",
    "Golden Retriever Dog.webp",
    "Grandma 1.webp",
    "Grandma 2.webp",
    "Grandma 3.webp",
    "Grandpa 1.webp",
    "Grandpa 2.webp",
    "Grandpa 3.webp",
    "Hackerman.webp",
    "Headphones.webp",
    "Horse.webp",
    "House.webp",
    "Husky Dog.webp",
    "Linux.webp",
    "Man 1.webp",
    "Man 2.webp",
    "Man 3.webp",
    "Mouse.webp",
    "Music Note.webp",
    "Ninja.webp",
    "PC.webp",
    "Phone.webp",
    "Pig.webp",
    "Rabbit.webp",
    "Rat.webp",
    "Robot.webp",
    "Rose.webp",
    "Santa.webp",
    "Sea Monster.webp",
    "Seagull.webp",
    "Shark.webp",
    "Springbok.webp",
    "TV.webp",
    "Tabby Cat.webp",
    "Tiger.webp",
    "Tortie Cat.webp",
    "Toy Pom Dog.webp",
    "Tuxedo Cat.webp",
    "Warthog.webp",
    "Watermelon Slice.webp",
    "Watermelon.webp",
    "Web.webp",
    "Windows.webp",
    "Witch.webp",
    "Wizard.webp",
    "Woman 1.webp",
    "Woman 2.webp",
    "Woman 3.webp",
    "Zombie.webp",
];

#[derive(Serialize)]
pub struct Avatar {
    pub id: &'static str,
    pub name: &'static str,
    pub url: String,
}

pub fn url(id: &str) -> Option<String> {
    if !FILES.contains(&id) {
        return None;
    }
    let mut url = url::Url::parse(BASE_URL).expect("static avatar base URL");
    url.path_segments_mut().ok()?.pop_if_empty().push(id);
    Some(url.into())
}

pub fn catalog() -> Vec<Avatar> {
    FILES
        .iter()
        .map(|&id| Avatar {
            id,
            name: id.strip_suffix(".webp").unwrap_or(id),
            url: url(id).expect("catalog contains allowed avatar"),
        })
        .collect()
}

pub(super) fn validate(avatar: Option<String>) -> madari_model::Result<Option<String>> {
    match avatar {
        None => Ok(None),
        Some(id) if FILES.contains(&id.as_str()) => Ok(Some(id)),
        Some(_) => Err(super::invalid(
            "choose an image from the profile image catalog",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_has_unique_safe_encoded_urls() {
        let entries = catalog();
        let ids: std::collections::HashSet<_> = entries.iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), FILES.len());
        assert_eq!(
            url("Black Cat.webp").unwrap(),
            format!("{BASE_URL}Black%20Cat.webp")
        );
        for invalid in [
            "../Fox.webp",
            "https://example.com/a.webp",
            "Unknown.webp",
            "",
        ] {
            assert!(url(invalid).is_none());
            assert!(validate(Some(invalid.into())).is_err());
        }
    }
}
