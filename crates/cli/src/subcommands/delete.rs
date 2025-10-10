use std::io;

use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, database_identity, get_auth_header, y_or_n, AuthHeader};
use clap::{Arg, ArgMatches};
use http::StatusCode;
use itertools::Itertools as _;
use reqwest::Response;
use spacetimedb_client_api_messages::http::{DatabaseDeleteConfirmationResponse, DatabaseTree, DatabaseTreeNode};
use spacetimedb_lib::Hash;
use tokio::io::AsyncWriteExt as _;

pub fn cli() -> clap::Command {
    clap::Command::new("delete")
        .about("Deletes a SpacetimeDB database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database to delete"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(common_args::yes())
        .after_help("Run `spacetime help delete` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let database = args.get_one::<String>("database").unwrap();
    let force = args.get_flag("force");

    let identity = database_identity(&config, database, server).await?;
    let host_url = config.get_host_url(server)?;
    let request_path = format!("{host_url}/v1/database/{identity}");
    let auth_header = get_auth_header(&mut config, false, server, !force).await?;
    let client = reqwest::Client::new();

    let response = send_request(&client, &request_path, &auth_header, None).await?;
    match response.status() {
        StatusCode::PRECONDITION_REQUIRED => {
            let confirm = response.json::<DatabaseDeleteConfirmationResponse>().await?;
            println!("WARNING: Deleting the database {identity} will also delete its children!");
            if !force {
                print_database_tree_info(&confirm.database_tree).await?;
            }
            if y_or_n(force, "Do you want to proceed deleting above databases?")? {
                send_request(&client, &request_path, &auth_header, Some(confirm.confirmation_token))
                    .await?
                    .error_for_status()?;
            } else {
                println!("Aborting");
            }

            Ok(())
        }
        StatusCode::OK => Ok(()),
        _ => response.error_for_status().map(drop).map_err(Into::into),
    }
}

async fn send_request(
    client: &reqwest::Client,
    request_path: &str,
    auth: &AuthHeader,
    confirmation_token: Option<Hash>,
) -> Result<Response, reqwest::Error> {
    let mut builder = client.delete(request_path);
    builder = add_auth_header_opt(builder, auth);
    if let Some(token) = confirmation_token {
        builder = builder.query(&[("token", token.to_string())]);
    }
    builder.send().await
}

async fn print_database_tree_info(tree: &DatabaseTree) -> io::Result<()> {
    tokio::io::stdout()
        .write_all(as_termtree(tree).to_string().as_bytes())
        .await
}

fn as_termtree(tree: &DatabaseTree) -> termtree::Tree<String> {
    let mut stack: Vec<(&DatabaseTree, bool)> = vec![];
    stack.push((tree, false));

    let mut built: Vec<termtree::Tree<String>> = <_>::default();

    while let Some((node, visited)) = stack.pop() {
        if visited {
            let mut term_node = termtree::Tree::new(fmt_tree_node(&node.root));
            term_node.leaves = built.drain(built.len() - node.children.len()..).collect();
            term_node.leaves.reverse();
            built.push(term_node);
        } else {
            stack.push((node, true));
            stack.extend(node.children.iter().rev().map(|child| (child, false)));
        }
    }

    built
        .pop()
        .expect("database tree contains a root and we pushed it last")
}

fn fmt_tree_node(node: &DatabaseTreeNode) -> String {
    format!(
        "{}{}",
        node.database_identity,
        if node.database_names.is_empty() {
            <_>::default()
        } else {
            format!(": {}", node.database_names.iter().join(", "))
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_client_api_messages::http::{DatabaseTree, DatabaseTreeNode};
    use spacetimedb_lib::{sats::u256, Identity};

    #[test]
    fn render_termtree() {
        let tree = DatabaseTree {
            root: DatabaseTreeNode {
                database_identity: Identity::ONE,
                database_names: ["parent".into()].into(),
            },
            children: vec![
                DatabaseTree {
                    root: DatabaseTreeNode {
                        database_identity: Identity::from_u256(u256::new(2)),
                        database_names: ["child".into()].into(),
                    },
                    children: vec![
                        DatabaseTree {
                            root: DatabaseTreeNode {
                                database_identity: Identity::from_u256(u256::new(3)),
                                database_names: ["grandchild".into()].into(),
                            },
                            children: vec![],
                        },
                        DatabaseTree {
                            root: DatabaseTreeNode {
                                database_identity: Identity::from_u256(u256::new(5)),
                                database_names: [].into(),
                            },
                            children: vec![],
                        },
                    ],
                },
                DatabaseTree {
                    root: DatabaseTreeNode {
                        database_identity: Identity::from_u256(u256::new(4)),
                        database_names: ["sibling".into(), "bro".into()].into(),
                    },
                    children: vec![],
                },
            ],
        };
        pretty_assertions::assert_eq!(
            "\
0000000000000000000000000000000000000000000000000000000000000001: parent
├── 0000000000000000000000000000000000000000000000000000000000000004: bro, sibling
└── 0000000000000000000000000000000000000000000000000000000000000002: child
    ├── 0000000000000000000000000000000000000000000000000000000000000005
    └── 0000000000000000000000000000000000000000000000000000000000000003: grandchild
",
            &as_termtree(&tree).to_string()
        );
    }
}
