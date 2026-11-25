namespace Consumer
{
    public static class Usage
    {
        public static void Use(A.DbConnection db)
        {
            db.Reducers.Add(getHttpCode: "hello", httpResult: 42, player2D: true);
            db.Procedures.Greet((ctx, result) => { });
            db.Db.Person.Id.Find(2);
            new A.QueryBuilder().From.All().ToSql();
        }
    }
}
