use anyhow::anyhow;
use chrono::{NaiveDate, Utc};
use derive_builder::Builder;

#[allow(unused)]
#[derive(Debug, Builder)]
#[builder(pattern = "owned")]
#[builder(build_fn(private, name = "pbuild"))]
pub struct User {
    #[builder(setter(into), default)]
    name: String,
    #[builder(setter(into, strip_option), default)]
    email: Option<String>,
    #[builder(setter(custom))]
    dob: NaiveDate,
    #[builder(setter(skip))]
    age: u32,
    #[builder(default = "Vec::new()", setter(each(name = "skill", into)))]
    skills: Vec<String>,
}

impl UserBuilder {
    pub fn dob(mut self, value: &str) -> Self {
        self.dob = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok();
        self
    }

    pub fn build(self) -> anyhow::Result<User> {
        let mut user = self.pbuild()?;

        user.age = Utc::now()
            .date_naive()
            .years_since(user.dob)
            .ok_or_else(|| anyhow!("calculate age error"))?;
        Ok(user)
    }
}

fn main() -> anyhow::Result<()> {
    let user = UserBuilder::default()
        .name("Alice")
        .email("lign@awsome.com")
        .dob("1998-10-2")
        .skill("guitar")
        .skill("computer science")
        .build()?;
    println!("user: {:?}", user);
    Ok(())
}
