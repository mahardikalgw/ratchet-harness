use anyhow::Result;
use ratchet_mcp::client::McpClient;
use std::path::Path;

pub async fn connect(_project_dir: &Path, command: &str, args: Vec<String>) -> Result<()> {
    println!(
        "🔗 Connecting to MCP server: {} {}",
        command,
        args.join(" ")
    );

    let mut client = McpClient::connect_stdio(command, &args).await?;

    println!("✅ Connected to MCP server");

    if client.supports_tools() {
        println!("\n🛠️  Tools:");
        let tools = client.list_tools().await?;
        for tool in tools {
            println!("   - {}: {:?}", tool.name, tool.description);
        }
    }

    if client.supports_resources() {
        println!("\n📄 Resources:");
        let resources = client.list_resources().await?;
        for resource in resources {
            println!("   - {} ({})", resource.name, resource.uri);
        }
    }

    Ok(())
}
