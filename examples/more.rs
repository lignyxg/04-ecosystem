use derive_more::{Add, Display, From, Into};

#[derive(Display, PartialEq, From, Add, Into)]
pub struct MyInt(i32);

#[derive(PartialEq, From, Add, Display)]
pub enum MyEnum {
    #[display(fmt = "int: {_0}")]
    Int(i32),
    Uint(u32),
    #[display(fmt = "nothing")]
    Nothing,
}

fn main() -> anyhow::Result<()> {
    let m_int = MyInt(17);
    let sum = m_int + 15.into();
    println!("sum: {}", sum);
    Ok(())
}
