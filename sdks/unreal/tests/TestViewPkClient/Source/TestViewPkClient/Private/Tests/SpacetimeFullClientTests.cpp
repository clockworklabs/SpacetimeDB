#include "Tests/SpacetimeFullClientTests.h"

#include "Tests/CommonTestFunctions.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "ModuleBindings/Tables/AllViewPkPlayersTable.g.h"
#include "ModuleBindings/Tables/SenderViewPkPlayersATable.g.h"

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

bool FViewPkQueryBuilderDirectSourcesTest::RunTest(const FString& Parameters)
{
    FQueryBuilder Q;

    TestEqual(
        TEXT("all_view_pk_players sql"),
        ToFString(Q.From.AllViewPkPlayers().into_sql()),
        TEXT("SELECT * FROM \"all_view_pk_players\"")
    );
    TestEqual(
        TEXT("sender_view_pk_players_a sql"),
        ToFString(Q.From.SenderViewPkPlayersA().into_sql()),
        TEXT("SELECT * FROM \"sender_view_pk_players_a\"")
    );
    TestEqual(
        TEXT("sender_view_pk_players_b sql"),
        ToFString(Q.From.SenderViewPkPlayersB().into_sql()),
        TEXT("SELECT * FROM \"sender_view_pk_players_b\"")
    );
    TestEqual(
        TEXT("view_pk_membership sql"),
        ToFString(Q.From.ViewPkMembership().into_sql()),
        TEXT("SELECT * FROM \"view_pk_membership\"")
    );
    TestEqual(
        TEXT("view_pk_membership_secondary sql"),
        ToFString(Q.From.ViewPkMembershipSecondary().into_sql()),
        TEXT("SELECT * FROM \"view_pk_membership_secondary\"")
    );
    TestEqual(
        TEXT("view_pk_player sql"),
        ToFString(Q.From.ViewPkPlayer().into_sql()),
        TEXT("SELECT * FROM \"view_pk_player\"")
    );

    FTypedSubscriptionBuilder Typed(nullptr);
    Typed.AddQuery([](const FQueryBuilder& Query)
    {
        return Query.From.AllViewPkPlayers();
    }).AddQuery([](const FQueryBuilder& Query)
    {
        return Query.From.ViewPkPlayer().Where([](const FViewPkPlayerCols& Cols)
        {
            return Cols.Id.Eq(static_cast<uint64>(1));
        });
    });

    TestTrue(TEXT("typed query builder accepted pk view sources"), true);
    return true;
}

bool FViewPkQueryBuilderSemijoinTest::RunTest(const FString& Parameters)
{
    FQueryBuilder Q;

    const FString MembershipJoinSql = ToFString(
        Q.From.ViewPkMembership().RightSemijoin(Q.From.AllViewPkPlayers(), [](const FViewPkMembershipIxCols& Membership, const FViewPkPlayerIxCols& Player)
        {
            return Membership.PlayerId.Eq(Player.Id);
        }).into_sql()
    );

    const FString SenderViewsJoinSql = ToFString(
        Q.From.SenderViewPkPlayersA().RightSemijoin(Q.From.SenderViewPkPlayersB(), [](const FViewPkPlayerIxCols& LeftView, const FViewPkPlayerIxCols& RightView)
        {
            return LeftView.Id.Eq(RightView.Id);
        }).into_sql()
    );

    TestEqual(
        TEXT("membership to all_view_pk_players semijoin sql"),
        MembershipJoinSql,
        TEXT("SELECT \"all_view_pk_players\".* FROM \"view_pk_membership\" JOIN \"all_view_pk_players\" ON \"view_pk_membership\".\"player_id\" = \"all_view_pk_players\".\"id\"")
    );

    TestEqual(
        TEXT("sender views semijoin sql"),
        SenderViewsJoinSql,
        TEXT("SELECT \"sender_view_pk_players_b\".* FROM \"sender_view_pk_players_a\" JOIN \"sender_view_pk_players_b\" ON \"sender_view_pk_players_a\".\"id\" = \"sender_view_pk_players_b\".\"id\"")
    );

    return true;
}

bool FViewPkSubscribeAllTablesTest::RunTest(const FString& Parameters)
{
    const TArray<FString> Sql = FQueryBuilder::AllTablesSqlQueries();

    TestEqual(TEXT("all tables count"), Sql.Num(), 6);
    TestTrue(TEXT("all tables include all_view_pk_players"), Sql.Contains(TEXT("SELECT * FROM \"all_view_pk_players\"")));
    TestTrue(TEXT("all tables include sender_view_pk_players_a"), Sql.Contains(TEXT("SELECT * FROM \"sender_view_pk_players_a\"")));
    TestTrue(TEXT("all tables include sender_view_pk_players_b"), Sql.Contains(TEXT("SELECT * FROM \"sender_view_pk_players_b\"")));
    TestTrue(TEXT("all tables include view_pk_membership"), Sql.Contains(TEXT("SELECT * FROM \"view_pk_membership\"")));
    TestTrue(TEXT("all tables include view_pk_membership_secondary"), Sql.Contains(TEXT("SELECT * FROM \"view_pk_membership_secondary\"")));
    TestTrue(TEXT("all tables include view_pk_player"), Sql.Contains(TEXT("SELECT * FROM \"view_pk_player\"")));

    return true;
}

bool FViewPkRuntimeUpdatePairingTest::RunTest(const FString& Parameters)
{
    const FString RuntimeTestName = TEXT("ViewPkRuntimeUpdatePairing");

    if (!ValidateParameterConfig(this))
    {
        return false;
    }

    UViewPkRuntimeHandler* Handler = CreateTestHandler<UViewPkRuntimeHandler>();
    Handler->Counter->Register(TEXT("subscription_applied"));
    Handler->Counter->Register(TEXT("all_view_pk_players_insert"));
    Handler->Counter->Register(TEXT("all_view_pk_players_update"));
    Handler->Counter->Register(TEXT("sender_view_pk_players_a_insert"));
    Handler->Counter->Register(TEXT("sender_view_pk_players_a_update"));

    ConnectThen(Handler->Counter, RuntimeTestName, [this, Handler](UDbConnection* Conn)
    {
        Conn->Db->AllViewPkPlayers->OnInsert.AddDynamic(Handler, &UViewPkRuntimeHandler::OnAllViewPkPlayersInsert);
        Conn->Db->AllViewPkPlayers->OnUpdate.AddDynamic(Handler, &UViewPkRuntimeHandler::OnAllViewPkPlayersUpdate);
        Conn->Db->AllViewPkPlayers->OnDelete.AddDynamic(Handler, &UViewPkRuntimeHandler::OnAllViewPkPlayersDelete);

        Conn->Db->SenderViewPkPlayersA->OnInsert.AddDynamic(Handler, &UViewPkRuntimeHandler::OnSenderViewPkPlayersAInsert);
        Conn->Db->SenderViewPkPlayersA->OnUpdate.AddDynamic(Handler, &UViewPkRuntimeHandler::OnSenderViewPkPlayersAUpdate);
        Conn->Db->SenderViewPkPlayersA->OnDelete.AddDynamic(Handler, &UViewPkRuntimeHandler::OnSenderViewPkPlayersADelete);

        UTestHelperDelegates* SubHelper = NewObject<UTestHelperDelegates>();
        SubHelper->AddToRoot();

        SubHelper->OnSubscriptionApplied = [this, Handler, Conn](FSubscriptionEventContext Ctx)
        {
            if (Conn->Db->AllViewPkPlayers->Count() != 0 || Conn->Db->SenderViewPkPlayersA->Count() != 0)
            {
                Handler->Counter->MarkFailure(TEXT("subscription_applied"), TEXT("Expected empty view caches before reducers"));
                Handler->Counter->Abort();
                return;
            }

            Handler->Counter->MarkSuccess(TEXT("subscription_applied"));

            Ctx.Reducers->InsertViewPkPlayer(Handler->ExpectedId, Handler->InitialName);
            Ctx.Reducers->InsertViewPkMembership(10, Handler->ExpectedId);
            Ctx.Reducers->UpdateViewPkPlayer(Handler->ExpectedId, Handler->UpdatedName);
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

        Conn->SubscriptionBuilder()
            ->OnApplied(AppliedDelegate)
            ->OnError(ErrorDelegate)
            ->AddQuery([](const FQueryBuilder& Q)
            {
                return Q.From.AllViewPkPlayers();
            })
            .AddQuery([](const FQueryBuilder& Q)
            {
                return Q.From.SenderViewPkPlayersA();
            })
            .Subscribe();
    });

    ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, RuntimeTestName, Handler->Counter, FPlatformTime::Seconds()));
    return true;
}
