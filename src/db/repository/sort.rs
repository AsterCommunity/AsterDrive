//! Shared repository helpers for applying whitelisted sort options.

use crate::api::pagination::SortOrder;
use sea_orm::{ColumnTrait, QueryOrder};

pub fn order_by_column<Q, C>(query: Q, column: C, order: SortOrder) -> Q
where
    Q: QueryOrder,
    C: ColumnTrait,
{
    match order {
        SortOrder::Asc => query.order_by_asc(column),
        SortOrder::Desc => query.order_by_desc(column),
    }
}

pub fn order_by_column_with_id<E, C, I>(query: E, column: C, order: SortOrder, id_column: I) -> E
where
    E: QueryOrder,
    C: ColumnTrait,
    I: ColumnTrait,
{
    order_by_id(order_by_column(query, column, order), id_column, order)
}

pub fn order_by_id<Q, I>(query: Q, id_column: I, order: SortOrder) -> Q
where
    Q: QueryOrder,
    I: ColumnTrait,
{
    match order {
        SortOrder::Asc => query.order_by_asc(id_column),
        SortOrder::Desc => query.order_by_desc(id_column),
    }
}
