use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rename column from count to tokens and change type from integer to double precision
        manager
            .alter_table(
                Table::alter()
                    .table(AccessLog::Table)
                    .rename_column(AccessLog::Count, AccessLog::Tokens)
                    .to_owned(),
            )
            .await?;

        // Change column type from integer to double precision
        manager
            .alter_table(
                Table::alter()
                    .table(AccessLog::Table)
                    .modify_column(ColumnDef::new(AccessLog::Tokens).double().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Change column type back to integer
        manager
            .alter_table(
                Table::alter()
                    .table(AccessLog::Table)
                    .modify_column(ColumnDef::new(AccessLog::Tokens).integer().null())
                    .to_owned(),
            )
            .await?;

        // Rename column back from tokens to count
        manager
            .alter_table(
                Table::alter()
                    .table(AccessLog::Table)
                    .rename_column(AccessLog::Tokens, AccessLog::Count)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum AccessLog {
    Table,
    Count,
    Tokens,
}
