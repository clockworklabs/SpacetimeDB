#include "Tests/SpacetimeFullClientTests.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"

namespace
{
static FString ToFString(const std::string& InSql)
{
    return FString(UTF8_TO_TCHAR(InSql.c_str()));
}
}

bool FViewQueryBuilderDirectSourcesTest::RunTest(const FString& Parameters)
{
    FQueryBuilder Q;

    const FString MyPlayerSql = ToFString(Q.From.MyPlayer().into_sql());
    const FString MyPlayerAndLevelSql = ToFString(Q.From.MyPlayerAndLevel().into_sql());
    const FString PlayersAtLevel0Sql = ToFString(Q.From.PlayersAtLevel0().into_sql());
    const FString NearbyPlayersFilteredSql = ToFString(
        Q.From.NearbyPlayers().Where([](const FPlayerLocationCols& Cols)
        {
            return Cols.Active.Eq(true);
        }).into_sql()
    );

    TestEqual(TEXT("my_player sql"), MyPlayerSql, TEXT("SELECT * FROM \"my_player\""));
    TestEqual(TEXT("my_player_and_level sql"), MyPlayerAndLevelSql, TEXT("SELECT * FROM \"my_player_and_level\""));
    TestEqual(TEXT("players_at_level_0 sql"), PlayersAtLevel0Sql, TEXT("SELECT * FROM \"players_at_level_0\""));
    TestEqual(
        TEXT("nearby_players filtered sql"),
        NearbyPlayersFilteredSql,
        TEXT("SELECT * FROM \"nearby_players\" WHERE (\"nearby_players\".\"active\" = TRUE)")
    );

    FTypedSubscriptionBuilder Typed(nullptr);
    Typed.AddQuery([](const FQueryBuilder& Query)
    {
        return Query.From.MyPlayer();
    }).AddQuery([](const FQueryBuilder& Query)
    {
        return Query.From.PlayersAtLevel0().Where([](const FPlayerCols& Cols)
        {
            return Cols.EntityId.Eq(static_cast<uint64>(7));
        });
    });

    TestTrue(TEXT("typed query builder accepted view sources"), true);
    return true;
}

bool FViewSubscribeAllTablesTest::RunTest(const FString& Parameters)
{
    const TArray<FString> Sql = FQueryBuilder::AllTablesSqlQueries();

    TestEqual(TEXT("all tables count"), Sql.Num(), 6);
    TestTrue(TEXT("all tables include player"), Sql.Contains(TEXT("SELECT * FROM \"player\"")));
    TestTrue(TEXT("all tables include player_level"), Sql.Contains(TEXT("SELECT * FROM \"player_level\"")));
    TestTrue(TEXT("all tables include my_player"), Sql.Contains(TEXT("SELECT * FROM \"my_player\"")));
    TestTrue(TEXT("all tables include my_player_and_level"), Sql.Contains(TEXT("SELECT * FROM \"my_player_and_level\"")));
    TestTrue(TEXT("all tables include nearby_players"), Sql.Contains(TEXT("SELECT * FROM \"nearby_players\"")));
    TestTrue(TEXT("all tables include players_at_level_0"), Sql.Contains(TEXT("SELECT * FROM \"players_at_level_0\"")));

    return true;
}
