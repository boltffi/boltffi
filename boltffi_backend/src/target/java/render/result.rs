use askama::Template as AskamaTemplate;

use crate::{
    core::{Error, FilePath, GeneratedFile, Result},
    target::java::{JavaFile, JavaPackage, JavaVersion, syntax::TypeIdentifier},
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/result.java", escape = "none")]
struct ResultTemplate<'package> {
    package: &'package JavaPackage,
}

pub struct ResultClass;

impl ResultClass {
    pub fn append(
        files: &mut Vec<GeneratedFile>,
        package: &JavaPackage,
        version: JavaVersion,
    ) -> Result<()> {
        let file = JavaFile::parse_for("BoltFFIResult", version)?;
        let path = FilePath::new(file.path(package))?;
        if files.iter().any(|generated| generated.path() == &path) {
            return Err(Error::JavaNameCollision {
                scope: package.to_string(),
                name: file.to_string(),
            });
        }
        files.push(GeneratedFile::new(
            path,
            ResultTemplate { package }.render()?,
        ));
        Ok(())
    }

    pub fn type_name(version: JavaVersion) -> TypeIdentifier {
        TypeIdentifier::known("BoltFFIResult", version)
    }
}
