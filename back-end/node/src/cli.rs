use crate::api::node::Node;
use errors::AppError;
use log::info;

pub async fn handle_cli_args(node: &Node, args: Vec<String>) -> Result<(), AppError> {
    if args.is_empty() {
        return Ok(());
    }

    match args[0].as_str() {
        "create-space" => {
            if args.len() < 2 {
                eprintln!("Usage: create-space <directory>");
                return Err(AppError::Config("Missing directory argument".to_string()));
            }
            let dir = &args[1];
            info!("Creating space for directory: {}", dir);
            node.create_space(dir).await?;
            println!("Space created successfully for directory: {}", dir);
        }
        "list-spaces" => {
            let spaces = node.list_spaces().await?;
            if spaces.is_empty() {
                println!("No spaces found.");
            } else {
                println!("Found {} spaces:", spaces.len());
                for space in spaces {
                    println!("  - Key: {}, Location: {}", space.key, space.location);
                }
            }
        }
        "list-events" => {
            let limit = args.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(10);
            let events = node.get_recent_events(limit).await?;
            if events.is_empty() {
                println!("No events found.");
            } else {
                println!("Found {} recent events:", events.len());
                for event in events {
                    println!("  - {}: {} ({})", 
                        event.event_type, 
                        event.file_path, 
                        event.timestamp.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
        "events-for-space" => {
            if args.len() < 2 {
                eprintln!("Usage: events-for-space <space_key>");
                return Err(AppError::Config("Missing space_key argument".to_string()));
            }
            let space_key = &args[1];
            let events = node.get_events_for_space(space_key).await?;
            if events.is_empty() {
                println!("No events found for space: {}", space_key);
            } else {
                println!("Found {} events for space {}:", events.len(), space_key);
                for event in events {
                    println!("  - {}: {} ({})", 
                        event.event_type, 
                        event.file_path, 
                        event.timestamp.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
        "files-for-space" => {
            if args.len() < 2 {
                eprintln!("Usage: files-for-space <space_key>");
                return Err(AppError::Config("Missing space_key argument".to_string()));
            }
            let space_key = &args[1];
            let files = node.get_file_metadata_for_space(space_key).await?;
            if files.is_empty() {
                println!("No files found for space: {}", space_key);
            } else {
                println!("Found {} files for space {}:", files.len(), space_key);
                for file in files {
                    println!("  - {} ({}) - {} bytes", 
                        file.file_path, 
                        file.file_extension.as_deref().unwrap_or("unknown"),
                        file.file_size.unwrap_or(0)
                    );
                }
            }
        }
        "knowledge-for-space" => {
            if args.len() < 2 {
                eprintln!("Usage: knowledge-for-space <space_key>");
                return Err(AppError::Config("Missing space_key argument".to_string()));
            }
            let space_key = &args[1];
            let knowledge = node.get_knowledge_nodes_for_space(space_key).await?;
            if knowledge.is_empty() {
                println!("No knowledge nodes found for space: {}", space_key);
            } else {
                println!("Found {} knowledge nodes for space {}:", knowledge.len(), space_key);
                for node in knowledge {
                    println!("  - {}: {} ({})", 
                        node.node_type, 
                        node.title.as_deref().unwrap_or("untitled"),
                        node.updated_at.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
        "search-knowledge" => {
            if args.len() < 2 {
                eprintln!("Usage: search-knowledge <query> [space_key]");
                return Err(AppError::Config("Missing query argument".to_string()));
            }
            let query = &args[1];
            let space_key = args.get(2).map(|s| s.as_str());
            let results = node.search_knowledge(query, space_key).await?;
            if results.is_empty() {
                println!("No knowledge found for query: {}", query);
            } else {
                println!("Found {} knowledge nodes for query '{}':", results.len(), query);
                for result in results {
                    println!("  - {}: {} ({})", 
                        result.node_type, 
                        result.title.as_deref().unwrap_or("untitled"),
                        result.updated_at.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
        _ => {
            eprintln!("Unknown command: {}", args[0]);
            eprintln!("Available commands:");
            eprintln!("  create-space <directory>  - Create a new space");
            eprintln!("  list-spaces               - List all spaces");
            eprintln!("  list-events [limit]       - List recent events (default: 10)");
            eprintln!("  events-for-space <key>    - List events for a specific space");
            eprintln!("  files-for-space <key>     - List files for a specific space");
            eprintln!("  knowledge-for-space <key> - List knowledge nodes for a specific space");
            eprintln!("  search-knowledge <query> [space_key] - Search knowledge repository");
        }
    }

    Ok(())
}
