use mousa::{
    catalog::Destination,
    control::{self, PageRequest, PlayerAction, QueueAction, Request},
    db::Track,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::wrapper::{Json, Parameters},
    model::{
        ListResourcesResult, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerConfig,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
struct MousaMcp;

fn page(offset: Option<usize>, limit: Option<usize>) -> PageRequest {
    PageRequest {
        offset: offset.unwrap_or(0),
        limit: limit.unwrap_or(100),
    }
}
fn call(request: Request) -> Result<Json<Value>, ErrorData> {
    control::call(request)
        .map(Json)
        .map_err(|e| ErrorData::invalid_params(e.message, Some(json!({"code":e.code}))))
}

#[derive(Deserialize, schemars::JsonSchema)]
struct PageArgs {
    offset: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct ListArgs {
    query: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct PlaylistArgs {
    id: i64,
    query: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct SearchLibraryArgs {
    query: String,
    playlist_id: Option<i64>,
    offset: Option<usize>,
    limit: Option<usize>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    query: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct DestinationArgs {
    destination: Destination,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct PositionArgs {
    position: f64,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct VolumeArgs {
    volume: f64,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct EnabledArgs {
    enabled: bool,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct TrackArgs {
    track: Track,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct PlayPlaylistArgs {
    id: i64,
    start_entry_id: Option<i64>,
    shuffle: Option<bool>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct IndexArgs {
    index: usize,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct ReplaceQueueArgs {
    tracks: Vec<Track>,
    start_index: Option<usize>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct TracksArgs {
    tracks: Vec<Track>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct IndicesArgs {
    indices: Vec<usize>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct MoveQueueArgs {
    indices: Vec<usize>,
    before: Option<usize>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct CreateArgs {
    name: String,
    google: Option<bool>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct UpdateArgs {
    id: i64,
    title: String,
    description: Option<String>,
    privacy: Option<String>,
    expected_revision: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct IdRevisionArgs {
    id: i64,
    expected_revision: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct AddTracksArgs {
    id: i64,
    tracks: Vec<Track>,
    expected_revision: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct EntryIdsArgs {
    ids: Vec<i64>,
    expected_revision: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct MoveEntriesArgs {
    ids: Vec<i64>,
    before: Option<i64>,
    expected_revision: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct RefreshArgs {
    id: i64,
    force: Option<bool>,
}

#[tool_router]
impl MousaMcp {
    #[tool(
        description = "Report whether Mousa is running and its account and player state.",
        annotations(read_only_hint = true)
    )]
    fn mousa_status(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Status)
    }
    #[tool(
        description = "List connected account names without exposing credentials.",
        annotations(read_only_hint = true)
    )]
    fn list_accounts(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Accounts)
    }
    #[tool(
        description = "List playlists with bounded pagination.",
        annotations(read_only_hint = true)
    )]
    fn list_playlists(
        &self,
        Parameters(a): Parameters<ListArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::ListPlaylists {
            query: a.query,
            page: page(a.offset, a.limit),
        })
    }
    #[tool(
        description = "Get playlist metadata and tracks by stable playlist ID.",
        annotations(read_only_hint = true)
    )]
    fn get_playlist(
        &self,
        Parameters(a): Parameters<PlaylistArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::GetPlaylist {
            id: a.id,
            query: a.query,
            page: page(a.offset, a.limit),
        })
    }
    #[tool(
        description = "Search library tracks, optionally within one playlist.",
        annotations(read_only_hint = true)
    )]
    fn search_library(
        &self,
        Parameters(a): Parameters<SearchLibraryArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::SearchLibrary {
            query: a.query,
            playlist_id: a.playlist_id,
            page: page(a.offset, a.limit),
        })
    }
    #[tool(
        description = "Search the YouTube Music catalog.",
        annotations(read_only_hint = true)
    )]
    fn search_catalog(
        &self,
        Parameters(a): Parameters<SearchArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::SearchCatalog { query: a.query })
    }
    #[tool(
        description = "Follow a catalog destination returned by another catalog call.",
        annotations(read_only_hint = true)
    )]
    fn get_catalog_page(
        &self,
        Parameters(a): Parameters<DestinationArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::GetCatalogPage {
            destination: a.destination,
        })
    }
    #[tool(
        description = "Get current playback state.",
        annotations(read_only_hint = true)
    )]
    fn get_player(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::GetPlayer)
    }
    #[tool(
        description = "Get the current queue with bounded pagination.",
        annotations(read_only_hint = true)
    )]
    fn get_queue(&self, Parameters(a): Parameters<PageArgs>) -> Result<Json<Value>, ErrorData> {
        call(Request::GetQueue {
            page: page(a.offset, a.limit),
        })
    }
    #[tool(description = "Resume playback.")]
    fn play(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::Play,
        })
    }
    #[tool(description = "Pause playback.")]
    fn pause(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::Pause,
        })
    }
    #[tool(description = "Stop playback.")]
    fn stop(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::Stop,
        })
    }
    #[tool(description = "Advance to the next track.")]
    fn next_track(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::Next,
        })
    }
    #[tool(description = "Return to the previous track.")]
    fn previous_track(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::Previous,
        })
    }
    #[tool(description = "Seek to an absolute position in seconds.")]
    fn seek(&self, Parameters(a): Parameters<PositionArgs>) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::Seek {
                position: a.position,
            },
        })
    }
    #[tool(description = "Set playback volume from 0 through 100.")]
    fn set_volume(&self, Parameters(a): Parameters<VolumeArgs>) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::SetVolume { volume: a.volume },
        })
    }
    #[tool(description = "Enable or disable shuffle.")]
    fn set_shuffle(
        &self,
        Parameters(a): Parameters<EnabledArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::SetShuffle { enabled: a.enabled },
        })
    }
    #[tool(description = "Enable or disable repeat.")]
    fn set_repeat(&self, Parameters(a): Parameters<EnabledArgs>) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::SetRepeat { enabled: a.enabled },
        })
    }
    #[tool(description = "Replace the queue with and play one explicit track.")]
    fn play_track(&self, Parameters(a): Parameters<TrackArgs>) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::PlayTrack { track: a.track },
        })
    }
    #[tool(description = "Play a playlist by ID, optionally beginning at an entry ID.")]
    fn play_playlist(
        &self,
        Parameters(a): Parameters<PlayPlaylistArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::PlayPlaylist {
                id: a.id,
                start_entry_id: a.start_entry_id,
                shuffle: a.shuffle.unwrap_or(false),
            },
        })
    }
    #[tool(description = "Jump to a zero-based queue index.")]
    fn play_queue_index(
        &self,
        Parameters(a): Parameters<IndexArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::Player {
            action: PlayerAction::PlayQueueIndex { index: a.index },
        })
    }
    #[tool(description = "Replace the queue and select its starting index.")]
    fn replace_queue(
        &self,
        Parameters(a): Parameters<ReplaceQueueArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::Queue {
            action: QueueAction::Replace {
                tracks: a.tracks,
                start_index: a.start_index.unwrap_or(0),
            },
        })
    }
    #[tool(description = "Append tracks to the queue.")]
    fn append_to_queue(
        &self,
        Parameters(a): Parameters<TracksArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::Queue {
            action: QueueAction::Append { tracks: a.tracks },
        })
    }
    #[tool(
        description = "Remove zero-based indices from the queue.",
        annotations(destructive_hint = true)
    )]
    fn remove_from_queue(
        &self,
        Parameters(a): Parameters<IndicesArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::Queue {
            action: QueueAction::Remove { indices: a.indices },
        })
    }
    #[tool(description = "Move queue indices before another index or to the end.")]
    fn move_queue_items(
        &self,
        Parameters(a): Parameters<MoveQueueArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::Queue {
            action: QueueAction::Move {
                indices: a.indices,
                before: a.before,
            },
        })
    }
    #[tool(
        description = "Remove every item from the queue.",
        annotations(destructive_hint = true)
    )]
    fn clear_queue(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::Queue {
            action: QueueAction::Clear,
        })
    }
    #[tool(description = "Create a local or connected Google playlist.")]
    fn create_playlist(
        &self,
        Parameters(a): Parameters<CreateArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::CreatePlaylist {
            name: a.name,
            google: a.google.unwrap_or(false),
        })
    }
    #[tool(description = "Update playlist metadata, optionally requiring an expected revision.")]
    fn update_playlist(
        &self,
        Parameters(a): Parameters<UpdateArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::UpdatePlaylist {
            id: a.id,
            title: a.title,
            description: a.description.unwrap_or_default(),
            privacy: a.privacy,
            expected_revision: a.expected_revision,
        })
    }
    #[tool(
        description = "Permanently delete a playlist by exact ID.",
        annotations(destructive_hint = true)
    )]
    fn delete_playlist(
        &self,
        Parameters(a): Parameters<IdRevisionArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::DeletePlaylist {
            id: a.id,
            expected_revision: a.expected_revision,
        })
    }
    #[tool(description = "Add tracks to a playlist in one operation.")]
    fn add_playlist_tracks(
        &self,
        Parameters(a): Parameters<AddTracksArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::AddPlaylistTracks {
            id: a.id,
            tracks: a.tracks,
            expected_revision: a.expected_revision,
        })
    }
    #[tool(
        description = "Remove playlist entries by exact entry IDs.",
        annotations(destructive_hint = true)
    )]
    fn remove_playlist_entries(
        &self,
        Parameters(a): Parameters<EntryIdsArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::RemovePlaylistEntries {
            ids: a.ids,
            expected_revision: a.expected_revision,
        })
    }
    #[tool(description = "Move playlist entry IDs before another entry or to the end.")]
    fn move_playlist_entries(
        &self,
        Parameters(a): Parameters<MoveEntriesArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::MovePlaylistEntries {
            ids: a.ids,
            before: a.before,
            expected_revision: a.expected_revision,
        })
    }
    #[tool(description = "Refresh the connected library.")]
    fn refresh_library(&self) -> Result<Json<Value>, ErrorData> {
        call(Request::RefreshLibrary)
    }
    #[tool(description = "Refresh a playlist by exact ID.")]
    fn refresh_playlist(
        &self,
        Parameters(a): Parameters<RefreshArgs>,
    ) -> Result<Json<Value>, ErrorData> {
        call(Request::RefreshPlaylist {
            id: a.id,
            force: a.force.unwrap_or(false),
        })
    }
}

#[tool_handler]
impl ServerHandler for MousaMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_instructions("Control the running Mousa desktop application. Read state before destructive mutations and use playlist revisions when available.")
    }

    #[allow(clippy::field_reassign_with_default)] // The SDK result is non-exhaustive.
    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let mut result = ListResourcesResult::default();
        result.resources = [
            ("mousa://player", "player", "Current player state"),
            ("mousa://queue", "queue", "Current playback queue"),
            ("mousa://library/playlists", "playlists", "Mousa playlists"),
            ("mousa://accounts", "accounts", "Connected account names"),
            (
                "mousa://sync",
                "sync",
                "Application and synchronization availability",
            ),
        ]
        .into_iter()
        .map(|(uri, name, description)| {
            Resource::new(uri, name)
                .with_description(description)
                .with_mime_type("application/json")
        })
        .collect();
        Ok(result)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let command = match request.uri.as_str() {
            "mousa://player" => Request::GetPlayer,
            "mousa://queue" => Request::GetQueue {
                page: page(Some(0), Some(500)),
            },
            "mousa://library/playlists" => Request::ListPlaylists {
                query: None,
                page: page(Some(0), Some(500)),
            },
            "mousa://accounts" => Request::Accounts,
            "mousa://sync" => Request::Status,
            _ => {
                return Err(ErrorData::invalid_params(
                    "Unknown Mousa resource",
                    Some(json!({"uri":request.uri})),
                ));
            }
        };
        let value = control::call(command)
            .map_err(|e| ErrorData::invalid_params(e.message, Some(json!({"code":e.code}))))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| "null".into()),
                request.uri,
            )
            .with_mime_type("application/json"),
        ])
        .into())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = MousaMcp.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_the_v1_tool_contract() {
        let tools = MousaMcp::tool_router().list_all();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        for expected in [
            "mousa_status",
            "list_accounts",
            "list_playlists",
            "get_playlist",
            "search_library",
            "search_catalog",
            "get_catalog_page",
            "get_player",
            "get_queue",
            "play",
            "pause",
            "stop",
            "next_track",
            "previous_track",
            "seek",
            "set_volume",
            "set_shuffle",
            "set_repeat",
            "play_track",
            "play_playlist",
            "play_queue_index",
            "replace_queue",
            "append_to_queue",
            "remove_from_queue",
            "move_queue_items",
            "clear_queue",
            "create_playlist",
            "update_playlist",
            "delete_playlist",
            "add_playlist_tracks",
            "remove_playlist_entries",
            "move_playlist_entries",
            "refresh_library",
            "refresh_playlist",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
        assert_eq!(names.len(), 34);
        for destructive in [
            "delete_playlist",
            "remove_playlist_entries",
            "remove_from_queue",
            "clear_queue",
        ] {
            let tool = tools.iter().find(|tool| tool.name == destructive).unwrap();
            assert_eq!(
                tool.annotations.as_ref().and_then(|a| a.destructive_hint),
                Some(true)
            );
        }
    }
}
