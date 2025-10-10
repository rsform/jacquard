use super::sources::{
    AtProtoSource, GitSource, HttpSource, JsonFileSource, LocalSource, SlicesSource, Source,
    SourceType,
};
use miette::{Result, miette};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub output: OutputConfig,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub lexicons_dir: PathBuf,
    pub codegen_dir: PathBuf,
    pub root_module: Option<String>,
}

impl Config {
    pub fn from_kdl(text: &str) -> Result<Self> {
        let doc = text
            .parse::<kdl::KdlDocument>()
            .map_err(|e| miette!("Failed to parse KDL: {}", e))?;

        let mut output: Option<OutputConfig> = None;
        let mut sources = Vec::new();

        for node in doc.nodes() {
            match node.name().value() {
                "output" => {
                    if output.is_some() {
                        return Err(miette!("Multiple output blocks found"));
                    }
                    output = Some(parse_output(node)?);
                }
                "source" => {
                    sources.push(parse_source(node)?);
                }
                other => {
                    return Err(miette!("Unknown config node: {}", other));
                }
            }
        }

        let output = output.ok_or_else(|| miette!("Missing output block"))?;

        Ok(Config { output, sources })
    }
}

fn parse_output(node: &kdl::KdlNode) -> Result<OutputConfig> {
    let children = node
        .children()
        .ok_or_else(|| miette!("output block has no children"))?;

    let mut lexicons_dir: Option<PathBuf> = None;
    let mut codegen_dir: Option<PathBuf> = None;
    let mut root_module: Option<String> = None;

    for child in children.nodes() {
        match child.name().value() {
            "lexicons" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("lexicons expects a string value"))?;
                lexicons_dir = Some(PathBuf::from(val));
            }
            "codegen" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("codegen expects a string value"))?;
                codegen_dir = Some(PathBuf::from(val));
            }
            "root-module" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("root-module expects a string value"))?;
                root_module = Some(val.to_string());
            }
            other => {
                return Err(miette!("Unknown output field: {}", other));
            }
        }
    }

    Ok(OutputConfig {
        lexicons_dir: lexicons_dir.ok_or_else(|| miette!("Missing lexicons directory"))?,
        codegen_dir: codegen_dir.ok_or_else(|| miette!("Missing codegen directory"))?,
        root_module,
    })
}

fn parse_source(node: &kdl::KdlNode) -> Result<Source> {
    let name = node
        .entries()
        .get(0)
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| miette!("source expects a name as first argument"))?
        .to_string();

    let type_str = node
        .get("type")
        .and_then(|v| v.as_string())
        .ok_or_else(|| miette!("source {} missing type attribute", name))?;

    let priority = node
        .get("priority")
        .and_then(|v| v.as_integer())
        .map(|i| i as i32);

    let children = node
        .children()
        .ok_or_else(|| miette!("source {} has no children", name))?;

    let source_type = match type_str {
        "atproto" => parse_atproto_source(children)?,
        "git" => parse_git_source(children)?,
        "http" => parse_http_source(children)?,
        "jsonfile" => parse_jsonfile_source(children)?,
        "local" => parse_local_source(children)?,
        "slices" => parse_slices_source(children)?,
        other => return Err(miette!("Unknown source type: {}", other)),
    };

    Ok(Source {
        name,
        source_type,
        explicit_priority: priority,
    })
}

fn parse_atproto_source(children: &kdl::KdlDocument) -> Result<SourceType> {
    let mut endpoint: Option<String> = None;
    let mut slice: Option<String> = None;

    for child in children.nodes() {
        match child.name().value() {
            "endpoint" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("endpoint expects a string value"))?;
                endpoint = Some(val.to_string());
            }
            "slice" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("slice expects a string value"))?;
                slice = Some(val.to_string());
            }
            other => {
                return Err(miette!("Unknown atproto source field: {}", other));
            }
        }
    }

    Ok(SourceType::AtProto(AtProtoSource {
        endpoint: endpoint.ok_or_else(|| miette!("Missing endpoint"))?,
        slice,
    }))
}

fn parse_git_source(children: &kdl::KdlDocument) -> Result<SourceType> {
    let mut repo: Option<String> = None;
    let mut git_ref: Option<String> = None;
    let mut pattern: Option<String> = None;

    for child in children.nodes() {
        match child.name().value() {
            "repo" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("repo expects a string value"))?;
                repo = Some(val.to_string());
            }
            "ref" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("ref expects a string value"))?;
                git_ref = Some(val.to_string());
            }
            "pattern" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("pattern expects a string value"))?;
                pattern = Some(val.to_string());
            }
            other => {
                return Err(miette!("Unknown git source field: {}", other));
            }
        }
    }

    Ok(SourceType::Git(GitSource {
        repo: repo.ok_or_else(|| miette!("Missing repo"))?,
        git_ref,
        pattern: pattern.unwrap_or_else(|| "**/*.json".to_string()),
    }))
}

fn parse_http_source(children: &kdl::KdlDocument) -> Result<SourceType> {
    let mut url: Option<String> = None;

    for child in children.nodes() {
        match child.name().value() {
            "url" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("url expects a string value"))?;
                url = Some(val.to_string());
            }
            other => {
                return Err(miette!("Unknown http source field: {}", other));
            }
        }
    }

    Ok(SourceType::Http(HttpSource {
        url: url.ok_or_else(|| miette!("Missing url"))?,
    }))
}

fn parse_jsonfile_source(children: &kdl::KdlDocument) -> Result<SourceType> {
    let mut path: Option<PathBuf> = None;

    for child in children.nodes() {
        match child.name().value() {
            "path" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("path expects a string value"))?;
                path = Some(PathBuf::from(val));
            }

            other => {
                return Err(miette!("Unknown jsonfile source field: {}", other));
            }
        }
    }

    Ok(SourceType::JsonFile(JsonFileSource {
        path: path.ok_or_else(|| miette!("Missing path"))?,
    }))
}

fn parse_slices_source(children: &kdl::KdlDocument) -> Result<SourceType> {
    let mut slice: Option<String> = None;

    for child in children.nodes() {
        match child.name().value() {
            "slice" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("slice expects a string value"))?;
                slice = Some(val.to_string());
            }
            other => {
                return Err(miette!("Unknown slices source field: {}", other));
            }
        }
    }

    Ok(SourceType::Slices(SlicesSource {
        slice: slice.ok_or_else(|| miette!("Missing slice"))?,
    }))
}

fn parse_local_source(children: &kdl::KdlDocument) -> Result<SourceType> {
    let mut path: Option<PathBuf> = None;
    let mut pattern: Option<String> = None;

    for child in children.nodes() {
        match child.name().value() {
            "path" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("path expects a string value"))?;
                path = Some(PathBuf::from(val));
            }
            "pattern" => {
                let val = child
                    .entries()
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| miette!("pattern expects a string value"))?;
                pattern = Some(val.to_string());
            }
            other => {
                return Err(miette!("Unknown local source field: {}", other));
            }
        }
    }

    Ok(SourceType::Local(LocalSource {
        path: path.ok_or_else(|| miette!("Missing path"))?,
        pattern,
    }))
}
