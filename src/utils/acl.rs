use std::path::Path;
use exacl::getfacl;

pub fn get_acl(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let acl = getfacl(dir)?;
    Ok(format!("{:?}", acl))
}