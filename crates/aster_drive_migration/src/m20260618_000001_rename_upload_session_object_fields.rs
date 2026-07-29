//! 数据库迁移：将 upload_sessions 的 S3 专用历史字段名改为 object upload 语义。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 只改当前 schema 的列名，baseline migration 保持创建当时的历史形态。
        // SQLite 每条 ALTER TABLE 只能稳定重命名一个列，因此这里不要合并两次 rename。
        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .rename_column(UploadSessions::S3TempKey, UploadSessions::ObjectTempKey)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .rename_column(
                        UploadSessions::S3MultipartId,
                        UploadSessions::ObjectMultipartId,
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .rename_column(UploadSessions::ObjectTempKey, UploadSessions::S3TempKey)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .rename_column(
                        UploadSessions::ObjectMultipartId,
                        UploadSessions::S3MultipartId,
                    )
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    S3TempKey,
    S3MultipartId,
    ObjectTempKey,
    ObjectMultipartId,
}
