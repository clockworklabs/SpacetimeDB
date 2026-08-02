#include "Tests/SpacetimeFullClientTests.h"

#include "Tests/CommonTestFunctions.h"
#include "Tests/TestHandler.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "ModuleBindings/Tables/PlayersAtLevel0Table.g.h"

namespace
{
static FString ToFString(const std::string& InSql)
{
    return FString(UTF8_TO_TCHAR(InSql.c_str()));
}

class FWaitForTestCounter : public IAutomationLatentCommand
{
public:
    FWaitForTestCounter(FAutomationTestBase& InTest, const FString& InTestName, TSharedPtr<FTestCounter> InCounter, double InStartTime)
        : Test(InTest)
        , TestName(InTestName)
        , Counter(MoveTemp(InCounter))
        , StartTime(InStartTime)
    {}

    virtual bool Update() override
    {
        const double Timeout = 90.0;
        const bool bStopped = Counter->IsAborted() || Counter->IsComplete() || (FPlatformTime::Seconds() - StartTime > Timeout);
        const bool bTimedOut = (FPlatformTime::Seconds() - StartTime > Timeout);

        if (bStopped)
        {
            ReportTestResult(Test, TestName, Counter, bTimedOut);
        }

        return bStopped;
    }

private:
    FAutomationTestBase& Test;
    FString TestName;
    TSharedPtr<FTestCounter> Counter;
    double StartTime = 0.0;
};
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

    USubscriptionBuilder* Builder = NewObject<USubscriptionBuilder>();
    Builder->AddQuery([](const FQueryBuilder& Query)
    {
        return Query.From.MyPlayer();
    })->AddQuery([](const FQueryBuilder& Query)
    {
        return Query.From.PlayersAtLevel0().Where([](const FPlayersAtLevel0Cols& Cols)
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

bool FViewBlueprintQueryBuilderFlowTest::RunTest(const FString& Parameters)
{
    FNearbyPlayersQuery Query = UQueryBuilderBlueprintLibrary::FromNearbyPlayers();
    TestEqual(TEXT("blueprint nearby players base sql"), Query.Sql, TEXT("SELECT * FROM \"nearby_players\""));

    const FBlueprintPredicate Active = UQueryBuilderBlueprintLibrary::BoolEqual(
        UQueryBuilderBlueprintLibrary::NearbyPlayersActive(Query),
        true);
    const FBlueprintPredicate MinX = UQueryBuilderBlueprintLibrary::Int32GreaterEqual(
        UQueryBuilderBlueprintLibrary::NearbyPlayersX(Query),
        0);
    Query = UQueryBuilderBlueprintLibrary::NearbyPlayersWhere(
        Query,
        UQueryBuilderBlueprintLibrary::And(Active, MinX));
    TestEqual(
        TEXT("blueprint nearby players filtered sql"),
        Query.Sql,
        TEXT("SELECT * FROM \"nearby_players\" WHERE ((\"nearby_players\".\"active\" = TRUE) AND (\"nearby_players\".\"x\" >= 0))")
    );

    USubscriptionBuilder* Builder = NewObject<USubscriptionBuilder>();
    USubscriptionHandle* Handle = Builder->AddNearbyPlayersQuery(Query)->Subscribe();
    TestNotNull(TEXT("blueprint nearby players handle"), Handle);
    TestEqual(TEXT("blueprint nearby players builder sql count"), Handle->GetQuerySqls().Num(), 1);
    TestEqual(TEXT("blueprint nearby players builder sql"), Handle->GetQuerySqls()[0], Query.Sql);

    return true;
}

bool FViewBlueprintQueryBuilderRuntimeTest::RunTest(const FString& Parameters)
{
    const FString RuntimeTestName = TEXT("ViewBlueprintQueryBuilderRuntime");

    if (!ValidateParameterConfig(this))
    {
        return false;
    }

    UViewBlueprintRuntimeHandler* Handler = CreateTestHandler<UViewBlueprintRuntimeHandler>();
    Handler->Counter->Register(TEXT("subscription_applied"));
    Handler->Counter->Register(TEXT("players_at_level_0_insert"));

    ConnectThen(Handler->Counter, RuntimeTestName, [this, Handler](UDbConnection* Conn)
    {
        Conn->Db->PlayersAtLevel0->OnInsert.AddDynamic(Handler, &UViewBlueprintRuntimeHandler::OnPlayersAtLevel0Insert);
        Conn->Db->PlayersAtLevel0->OnUpdate.AddDynamic(Handler, &UViewBlueprintRuntimeHandler::OnPlayersAtLevel0Update);
        Conn->Db->PlayersAtLevel0->OnDelete.AddDynamic(Handler, &UViewBlueprintRuntimeHandler::OnPlayersAtLevel0Delete);

        UTestHelperDelegates* SubHelper = NewObject<UTestHelperDelegates>();
        SubHelper->AddToRoot();

        SubHelper->OnSubscriptionApplied = [this, Handler, Conn](FSubscriptionEventContext Ctx)
        {
            if (Conn->Db->PlayersAtLevel0->Count() != 0)
            {
                Handler->Counter->MarkFailure(TEXT("subscription_applied"), TEXT("Expected empty players_at_level_0 cache before reducers"));
                Handler->Counter->Abort();
                return;
            }

            Handler->Counter->MarkSuccess(TEXT("subscription_applied"));
            Ctx.Reducers->InsertPlayer(Handler->ExpectedIdentity, static_cast<uint64>(0));
        };

        SubHelper->OnSubscriptionError = [Handler](FErrorContext Ctx)
        {
            Handler->Counter->MarkFailure(TEXT("subscription_applied"), FString::Printf(TEXT("Subscription error: %s"), *Ctx.Error));
            Handler->Counter->Abort();
        };

        FOnSubscriptionApplied AppliedDelegate;
        BIND_DELEGATE_SAFE(AppliedDelegate, SubHelper, UTestHelperDelegates, HandleSubscriptionApplied);

        FOnSubscriptionError ErrorDelegate;
        BIND_DELEGATE_SAFE(ErrorDelegate, SubHelper, UTestHelperDelegates, HandleSubscriptionError);

        FPlayersAtLevel0Query Query = UQueryBuilderBlueprintLibrary::FromPlayersAtLevel0();
        Query = UQueryBuilderBlueprintLibrary::PlayersAtLevel0Where(
            Query,
            UQueryBuilderBlueprintLibrary::IdentityEqual(
                UQueryBuilderBlueprintLibrary::PlayersAtLevel0Identity(Query),
                Handler->ExpectedIdentity));

        Conn->SubscriptionBuilder()
            ->OnApplied(AppliedDelegate)
            ->OnError(ErrorDelegate)
            ->AddPlayersAtLevel0Query(Query)
            ->Subscribe();
    });

    ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, RuntimeTestName, Handler->Counter, FPlatformTime::Seconds()));
    return true;
}
