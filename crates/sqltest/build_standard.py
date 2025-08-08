import glob
import os
import yaml
from dataclasses import dataclass
import shutil

base = os.path.dirname(__file__)
standard = os.path.join(base, 'standards', '2016')
tests_paths = glob.glob(os.path.join(standard, '**', '*.tests.yml'), recursive=True)
output_path = os.path.join(base, 'test', 'sql_2016')

shutil.rmtree(output_path)
os.mkdir(output_path)

mandatory = {}
with open(os.path.join(standard, 'features.yml'), 'r') as file:
    document = next(yaml.safe_load_all(file))

    mandatory = document['mandatory']


# print(mandatory)
# print(optional)


@dataclass
class Test:
    feature: str
    id: str
    sql: str
    desc: str


def comment_sql(sql: str):
    new = ""
    for x in sql.split("\n"):
        new = new + "# " + x + "\n"
    return new

# Patch the cases that are not supported or need some adjustments to run
def fix_sql(old_sql: str):
    sql = old_sql
    sql_lower = sql.lower()
    if sql_lower.startswith("select"):
        header = "query I"
        footer = "----\n"
    else:
        header = "statement ok"
        footer = ""

    if 'octets' in sql_lower:
        header = "# (UNSUPPORTED: issue 1) " + header
        sql = comment_sql(sql)
        footer = ""
    if 'characters' in sql_lower:
        header = "# (UNSUPPORTED: issue 1) " + header
        sql = comment_sql(sql)
        footer = ""
    if 'char varing' in sql_lower:
        header = "# (UNSUPPORTED: issue 2) " + header
        sql = comment_sql(sql)
        footer = ""
    if 'as ( c , d )' in sql_lower:
        header = "# (UNSUPPORTED: issue 3) " + header
        sql = comment_sql(sql)
        footer = ""
    if 'current_time' in sql_lower and not('current_timestamp' in sql_lower):
        header = "# (REPLACED: issue 4)\n" + header
        sql = sql.replace("CURRENT_TIME", "CURRENT_TIMESTAMP") 
    if 'when 2 , 2' in sql_lower:
        header = "# (UNSUPPORTED: issue 5) " + header
        sql = comment_sql(sql)
        footer = ""
    if "( cast ( '01:02:03' as time ) as timestamp )" in sql_lower:
        header = "# (WRONG: issue 6) " + header
        sql = comment_sql(sql)
        footer = ""
    if 'default current_path' in sql_lower:
         header = "skipif Postgres\n" + header
         sql = sql + " --NOT_REWRITE"
    if 'default system_user' in sql_lower:
         header = "skipif Postgres\n" + header
         sql = sql + " --NOT_REWRITE"
    if 'schema' in sql_lower:
         header = "onlyif Postgres\n" + header
    if 'cursor' in sql_lower:
        header = "onlyif Postgres\n" + header
    if 'type' in sql_lower:
        header = "onlyif Postgres\n" + header
    if 'open cur' in sql_lower:
        header = "onlyif Postgres\n" + header
    if 'close cur' in sql_lower:
        header = "onlyif Postgres\n" + header
    if 'role' in sql_lower:
        header = "onlyif Postgres\n" + header
    if "select" in sql_lower:
        if 'current_time' in sql_lower:
            sql += " = CURRENT_TIMESTAMP" 
        if 'current_date' in sql_lower:
            sql += " = CURRENT_DATE" 

    return "\n%s\n%s\n%s" % (header, sql, footer)

def write_sql(file: file, data: Test):
    if not (isinstance(data.sql, list)):
        data.sql = [data.sql]

    sql = ";\n".join(data.sql)
    file.write(fix_sql(sql))

def generate(data: Test):
    file_name = data.feature.replace("-", "_") + ".slt"
    file_name = os.path.join(output_path, file_name)
    print(file_name)
    if os.path.exists(file_name):
        with open(file_name, 'a') as file:
            write_sql(file, data)
    else:
        with open(file_name, 'w') as file:
            file.write("# %s: %s\n" % (data.feature, data.desc))
            write_sql(file, data)


total = 0
for file_path in tests_paths:
    print(file_path)
    with open(file_path, 'r') as file:
        documents = yaml.safe_load_all(file)
        for row in documents:
            # print(row)
            # print("\n")
            feature = row['feature']
            if feature in mandatory:
                desc = mandatory[feature]
            else:
                continue

            t = Test(feature=feature, id=row['id'], sql=row['sql'],  desc=desc)
            generate(t)
            total += 1
print(total)
