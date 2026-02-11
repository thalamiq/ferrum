use super::{query_builder, SearchEngine, SearchParameters};
use crate::Result;
use sqlx::PgConnection;

impl SearchEngine {
    pub(super) async fn resolve_sort_params(
        &self,
        conn: &mut PgConnection,
        resource_type: Option<&str>,
        params: &SearchParameters,
    ) -> Result<Vec<query_builder::ResolvedSort>> {
        use crate::db::search::parameter_lookup::SearchParamType as PT;
        use query_builder::{ResolvedSort, ResolvedSortKey, SearchModifier};

        if params.sort.is_empty() {
            return Ok(Vec::new());
        }

        let searched_type = resource_type.or_else(|| {
            if params.types.len() == 1 {
                Some(params.types[0].as_str())
            } else {
                None
            }
        });

        let mut out = Vec::new();
        for s in &params.sort {
            match s.param.as_str() {
                "_id" => {
                    out.push(ResolvedSort {
                        key: ResolvedSortKey::Id,
                        ascending: s.ascending,
                    });
                    continue;
                }
                "_lastUpdated" => {
                    out.push(ResolvedSort {
                        key: ResolvedSortKey::LastUpdated,
                        ascending: s.ascending,
                    });
                    continue;
                }
                _ => {}
            }

            let Some(rt) = searched_type else {
                return Err(crate::Error::Validation(
                    "Sorting by search parameters requires a single resource type".to_string(),
                ));
            };

            let Some(def) = self
                .param_cache
                .get_param_with_conn(conn, rt, &s.param)
                .await?
            else {
                return Err(crate::Error::Validation(format!(
                    "Unsupported _sort parameter: {}",
                    s.param
                )));
            };

            let modifier = match s.modifier.as_deref() {
                None => None,
                Some("text") => Some(SearchModifier::Text),
                Some(other) => {
                    return Err(crate::Error::Validation(format!(
                        "Unsupported _sort modifier: {}",
                        other
                    )));
                }
            };

            if matches!(modifier, Some(SearchModifier::Text))
                && !matches!(def.param_type, PT::Token | PT::Reference)
            {
                return Err(crate::Error::Validation(
                    "Sort modifier ':text' is only supported for reference and token parameters"
                        .to_string(),
                ));
            }

            if matches!(
                def.param_type,
                PT::Composite | PT::Special | PT::Content | PT::Text
            ) {
                return Err(crate::Error::Validation(format!(
                    "Sorting is not supported for search parameter type {:?}",
                    def.param_type
                )));
            }

            out.push(ResolvedSort {
                key: ResolvedSortKey::Param {
                    code: s.param.clone(),
                    param_type: def.param_type,
                    modifier,
                },
                ascending: s.ascending,
            });
        }

        Ok(out)
    }
}
