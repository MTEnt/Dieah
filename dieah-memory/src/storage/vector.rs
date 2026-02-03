//! Vector storage using LanceDB for semantic search

use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lance_arrow::FixedSizeListArrayExt;
use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::sync::Arc;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::memory::Memory;

const TABLE_NAME: &str = "memories";

/// Vector storage backend using LanceDB
pub struct VectorStorage {
    db: lancedb::Connection,
    dimensions: usize,
}

impl VectorStorage {
    /// Create a new vector storage
    pub async fn new(config: &Config) -> Result<Self> {
        let db = connect(config.vector_db_path().to_str().unwrap())
            .execute()
            .await
            .map_err(|e| Error::vector_db(e.to_string()))?;

        let storage = Self {
            db,
            dimensions: config.embedding_dimensions,
        };

        // Ensure table exists
        storage.ensure_table().await?;

        Ok(storage)
    }

    /// Get the schema for the memories table
    fn schema(&self) -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("scope", DataType::Utf8, false),
            Field::new("memory_type", DataType::Utf8, false),
            Field::new("agent_id", DataType::Utf8, true),
            Field::new("topic_id", DataType::Utf8, true),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.dimensions as i32,
                ),
                false,
            ),
        ])
    }

    /// Ensure the memories table exists
    async fn ensure_table(&self) -> Result<()> {
        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .map_err(|e| Error::vector_db(e.to_string()))?;

        if !tables.contains(&TABLE_NAME.to_string()) {
            // Create empty table with schema
            let schema = Arc::new(self.schema());
            
            // Create an empty batch with the schema
            let empty_batch = RecordBatch::new_empty(schema.clone());
            let batches = vec![empty_batch];
            let reader = RecordBatchIterator::new(batches.into_iter().map(Ok), schema);

            self.db
                .create_table(TABLE_NAME, Box::new(reader))
                .execute()
                .await
                .map_err(|e| Error::vector_db(e.to_string()))?;
        }

        Ok(())
    }

    /// Insert or update a memory in the vector store
    pub async fn upsert_memory(&self, memory: &Memory) -> Result<()> {
        let embedding = memory
            .embedding
            .as_ref()
            .ok_or_else(|| Error::vector_db("Memory has no embedding"))?;

        if embedding.len() != self.dimensions {
            return Err(Error::vector_db(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                embedding.len()
            )));
        }

        // First try to delete existing record
        let _ = self.delete_memory(memory.id).await;

        // Build arrays for the record batch
        let id_array = StringArray::from(vec![memory.id.to_string()]);
        let content_array = StringArray::from(vec![memory.content.clone()]);
        let scope_array = StringArray::from(vec![memory.scope.to_string()]);
        let type_array = StringArray::from(vec![memory.memory_type.to_string()]);
        let agent_id_array = StringArray::from(vec![memory.agent_id.clone()]);
        let topic_id_array = StringArray::from(vec![memory.topic_id.clone()]);

        // Build the vector array
        let values = Float32Array::from(embedding.clone());
        let vector_array = FixedSizeListArray::try_new_from_values(values, self.dimensions as i32)
            .map_err(|e: arrow_schema::ArrowError| Error::vector_db(e.to_string()))?;

        let schema = Arc::new(self.schema());
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array) as Arc<dyn Array>,
                Arc::new(content_array),
                Arc::new(scope_array),
                Arc::new(type_array),
                Arc::new(agent_id_array),
                Arc::new(topic_id_array),
                Arc::new(vector_array),
            ],
        )
        .map_err(|e| Error::vector_db(e.to_string()))?;

        let batches = vec![batch];
        let reader = RecordBatchIterator::new(batches.into_iter().map(Ok), schema);

        let table = self
            .db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| Error::vector_db(e.to_string()))?;

        table
            .add(Box::new(reader))
            .execute()
            .await
            .map_err(|e| Error::vector_db(e.to_string()))?;

        Ok(())
    }

    /// Delete a memory from the vector store
    pub async fn delete_memory(&self, id: Uuid) -> Result<()> {
        let table = self
            .db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| Error::vector_db(e.to_string()))?;

        table
            .delete(&format!("id = '{}'", id))
            .await
            .map_err(|e| Error::vector_db(e.to_string()))?;

        Ok(())
    }

    /// Search for similar memories
    pub async fn search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        min_score: f32,
        scope_filter: Option<&str>,
        agent_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let table = self
            .db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e: lancedb::Error| Error::vector_db(e.to_string()))?;

        let mut query = table
            .vector_search(query_embedding.to_vec())
            .map_err(|e: lancedb::Error| Error::vector_db(e.to_string()))?
            .limit(limit);

        // Build filter string
        let mut filters = Vec::new();
        if let Some(scope) = scope_filter {
            filters.push(format!("scope = '{}'", scope));
        }
        if let Some(agent_id) = agent_filter {
            filters.push(format!("agent_id = '{}'", agent_id));
        }

        if !filters.is_empty() {
            query = query.only_if(filters.join(" AND "));
        }

        let stream = query
            .execute()
            .await
            .map_err(|e: lancedb::Error| Error::vector_db(e.to_string()))?;

        let batches: Vec<RecordBatch> = stream
            .try_collect::<Vec<RecordBatch>>()
            .await
            .map_err(|e: lancedb::Error| Error::vector_db(e.to_string()))?;

        let mut search_results = Vec::new();

        for batch in batches {
            // Get column arrays
            let id_col: &Arc<dyn Array> = batch.column_by_name("id")
                .ok_or_else(|| Error::vector_db("Missing id column"))?;
            let content_col: &Arc<dyn Array> = batch.column_by_name("content")
                .ok_or_else(|| Error::vector_db("Missing content column"))?;
            let scope_col: &Arc<dyn Array> = batch.column_by_name("scope")
                .ok_or_else(|| Error::vector_db("Missing scope column"))?;
            let type_col: &Arc<dyn Array> = batch.column_by_name("memory_type")
                .ok_or_else(|| Error::vector_db("Missing memory_type column"))?;
            let distance_col: &Arc<dyn Array> = batch.column_by_name("_distance")
                .ok_or_else(|| Error::vector_db("Missing _distance column"))?;
            
            // Downcast to typed arrays
            let ids = id_col.as_any().downcast_ref::<StringArray>()
                .ok_or_else(|| Error::vector_db("id column is not StringArray"))?;
            let contents = content_col.as_any().downcast_ref::<StringArray>()
                .ok_or_else(|| Error::vector_db("content column is not StringArray"))?;
            let scopes = scope_col.as_any().downcast_ref::<StringArray>()
                .ok_or_else(|| Error::vector_db("scope column is not StringArray"))?;
            let types = type_col.as_any().downcast_ref::<StringArray>()
                .ok_or_else(|| Error::vector_db("memory_type column is not StringArray"))?;
            let distances = distance_col.as_any().downcast_ref::<Float32Array>()
                .ok_or_else(|| Error::vector_db("_distance column is not Float32Array"))?;

            for i in 0..batch.num_rows() {
                let distance = distances.value(i);
                // LanceDB returns L2 distance, convert to similarity score
                let score = 1.0 / (1.0 + distance);

                if score >= min_score {
                    search_results.push(SearchResult {
                        id: Uuid::parse_str(ids.value(i))
                            .map_err(|e| Error::vector_db(e.to_string()))?,
                        content: contents.value(i).to_string(),
                        scope: scopes.value(i).to_string(),
                        memory_type: types.value(i).to_string(),
                        score,
                    });
                }
            }
        }

        Ok(search_results)
    }
}

/// Result from a vector similarity search
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: Uuid,
    pub content: String,
    pub scope: String,
    pub memory_type: String,
    pub score: f32,
}

use futures::TryStreamExt;
