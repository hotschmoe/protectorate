H='Content-Type: application/json'
B=http://127.0.0.1:7471
echo '--- spawn'
curl -s -i -H "$H" -d '{"name":"alice","harness":"claude"}' $B/protectorate.v1.SleeveService/Spawn
echo; echo '--- dup'
curl -s -i -H "$H" -d '{"name":"alice"}' $B/protectorate.v1.SleeveService/Spawn
echo; echo '--- empty name'
curl -s -H "$H" -d '{}' $B/protectorate.v1.SleeveService/Spawn
echo; echo '--- bad field'
curl -s -H "$H" -d '{"nme":"x"}' $B/protectorate.v1.SleeveService/Spawn
echo; echo '--- list'
curl -s -H "$H" -d '{}' $B/protectorate.v1.SleeveService/List
echo; echo '--- GET list'
curl -s "$B/protectorate.v1.SleeveService/List?encoding=json&message=%7B%7D" -w ' [%{http_code}]'
echo; echo '--- stream (connect+json envelope)'
printf "\000\000\000\000\002{}" | timeout 2.5 curl -s -N -H "Content-Type: application/connect+json" --data-binary @- $B/protectorate.v1.EventService/Subscribe | tr -c "[:print:]\n" "|"
echo; echo '--- mcp'
curl -s $B/mcp/tools
