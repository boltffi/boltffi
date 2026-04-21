use boltffi::*;

use crate::enums::{Filter, Message, Priority};
use crate::records::{
    Address, Classroom, Person, Point, Polygon, SearchResult, Task, Team, UserProfile,
};

pub struct ConstructorCoverageMatrix {
    constructor_variant: String,
    summary: String,
    payload_checksum: u32,
    vector_count: u32,
}

#[export]
impl ConstructorCoverageMatrix {
    pub fn new() -> Self {
        Self::from_parts("new", "default", 0, 0)
    }

    pub fn with_scalar_mix(version: u32, enabled: bool, priority: Priority) -> Self {
        Self::from_parts(
            "with_scalar_mix",
            format!(
                "version={version};enabled={enabled};priority={}",
                Self::describe_priority(priority)
            ),
            0,
            0,
        )
    }

    pub fn with_string_and_bytes(label: String, payload: Vec<u8>) -> Self {
        let payload_checksum = Self::compute_payload_checksum(&payload);
        Self::from_parts(
            "with_string_and_bytes",
            format!("label={label};bytes={}", payload.len()),
            payload_checksum,
            payload.len() as u32,
        )
    }

    pub fn with_blittable_and_record(origin: Point, person: Person) -> Self {
        Self::from_parts(
            "with_blittable_and_record",
            format!(
                "origin={:.1}:{:.1};person={}#{}",
                origin.x, origin.y, person.name, person.age
            ),
            0,
            1,
        )
    }

    pub fn with_optional_profile_and_cursor(
        profile: Option<UserProfile>,
        next_cursor: Option<String>,
    ) -> Self {
        let profile_summary = profile
            .as_ref()
            .map(Self::summarize_profile)
            .unwrap_or_else(|| "profile=none".to_string());
        let cursor_summary = next_cursor
            .as_deref()
            .map(|cursor| format!("cursor={cursor}"))
            .unwrap_or_else(|| "cursor=none".to_string());
        Self::from_parts(
            "with_optional_profile_and_cursor",
            format!("{profile_summary};{cursor_summary}"),
            0,
            u32::from(profile.is_some()) + u32::from(next_cursor.is_some()),
        )
    }

    pub fn with_vectors_and_polygon(
        tags: Vec<String>,
        anchors: Vec<Point>,
        polygon: Polygon,
    ) -> Self {
        let anchor_count = anchors.len() as u32;
        let polygon_count = polygon.points.len() as u32;
        let joined_tags = tags.join("|");
        Self::from_parts(
            "with_vectors_and_polygon",
            format!("tags={joined_tags};anchors={anchor_count};polygon={polygon_count}"),
            0,
            tags.len() as u32 + anchor_count + polygon_count,
        )
    }

    pub fn with_collection_records(team: Team, classroom: Classroom, polygon: Polygon) -> Self {
        let member_count = team.members.len() as u32;
        let student_count = classroom.students.len() as u32;
        let polygon_count = polygon.points.len() as u32;
        Self::from_parts(
            "with_collection_records",
            format!(
                "team={};members={member_count};students={student_count};polygon={polygon_count}",
                team.name
            ),
            0,
            member_count + student_count + polygon_count,
        )
    }

    pub fn with_enum_mix(filter: Filter, message: Message, task: Task) -> Self {
        Self::from_parts(
            "with_enum_mix",
            format!(
                "filter={};message={};task={}#{}",
                Self::summarize_filter(&filter),
                Self::summarize_message(&message),
                task.title,
                Self::describe_priority(task.priority)
            ),
            0,
            1,
        )
    }

    pub fn with_everything(
        person: Person,
        address: Address,
        profile: UserProfile,
        search_result: SearchResult,
        payload: Vec<u8>,
        filter: Filter,
        tags: Vec<String>,
    ) -> Self {
        let payload_checksum = Self::compute_payload_checksum(&payload);
        let tag_count = tags.len() as u32;
        Self::from_parts(
            "with_everything",
            format!(
                "person={}#{};city={};profile={};query={};filter={};tags={}",
                person.name,
                person.age,
                address.city,
                Self::summarize_profile(&profile),
                search_result.query,
                Self::summarize_filter(&filter),
                tags.join("|")
            ),
            payload_checksum,
            tag_count + payload.len() as u32 + search_result.total,
        )
    }

    pub fn try_with_payload_and_search_result(
        payload: Vec<u8>,
        search_result: SearchResult,
        filter: Filter,
    ) -> Result<Self, String> {
        if payload.is_empty() {
            return Err("payload must not be empty".to_string());
        }

        let payload_checksum = Self::compute_payload_checksum(&payload);
        Ok(Self::from_parts(
            "try_with_payload_and_search_result",
            format!(
                "query={};cursor={};filter={}",
                search_result.query,
                search_result.next_cursor.as_deref().unwrap_or("none"),
                Self::summarize_filter(&filter)
            ),
            payload_checksum,
            payload.len() as u32 + search_result.total,
        ))
    }

    pub fn summarize_borrowed_inputs(
        &self,
        profile: &UserProfile,
        search_result: &SearchResult,
        filter: &Filter,
    ) -> String {
        format!(
            "{};query={};filter={}",
            Self::summarize_profile(profile),
            search_result.query,
            Self::summarize_filter(filter)
        )
    }

    pub fn constructor_variant(&self) -> String {
        self.constructor_variant.clone()
    }

    pub fn summary(&self) -> String {
        self.summary.clone()
    }

    pub fn payload_checksum(&self) -> u32 {
        self.payload_checksum
    }

    pub fn vector_count(&self) -> u32 {
        self.vector_count
    }
}

impl ConstructorCoverageMatrix {
    fn from_parts(
        constructor_variant: impl Into<String>,
        summary: impl Into<String>,
        payload_checksum: u32,
        vector_count: u32,
    ) -> Self {
        Self {
            constructor_variant: constructor_variant.into(),
            summary: summary.into(),
            payload_checksum,
            vector_count,
        }
    }

    fn compute_payload_checksum(payload: &[u8]) -> u32 {
        payload.iter().map(|byte| u32::from(*byte)).sum()
    }

    fn summarize_profile(profile: &UserProfile) -> String {
        let email = profile.email.as_deref().unwrap_or("none");
        let score = profile
            .score
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "none".to_string());
        format!("profile={}#{}#{email}#{score}", profile.name, profile.age)
    }

    fn summarize_filter(filter: &Filter) -> String {
        match filter {
            Filter::None => "none".to_string(),
            Filter::ByName { name } => format!("name:{name}"),
            Filter::ByRange { min, max } => format!("range:{min:.1}-{max:.1}"),
            Filter::ByTags { tags } => format!("tags:{}", tags.join("|")),
            Filter::ByGroups { groups } => format!("groups:{}", groups.len()),
            Filter::ByPoints { anchors } => format!("points:{}", anchors.len()),
        }
    }

    fn summarize_message(message: &Message) -> String {
        match message {
            Message::Text { body } => format!("text:{body}"),
            Message::Image { url, width, height } => format!("image:{url}#{width}x{height}"),
            Message::Ping => "ping".to_string(),
        }
    }

    fn describe_priority(priority: Priority) -> &'static str {
        match priority {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
            Priority::Critical => "critical",
        }
    }
}
