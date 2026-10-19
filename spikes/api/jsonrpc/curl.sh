B=http://127.0.0.1:7472
H='Content-Type: application/json'
echo '--- spawn'
curl -s -i -H "$H" -d '{"jsonrpc":"2.0","id":1,"method":"sleeve.spawn","params":{"name":"alice","harness":"claude"}}' $B/rpc
echo; echo '--- dup'
curl -s -H "$H" -d '{"jsonrpc":"2.0","id":2,"method":"sleeve.spawn","params":{"name":"alice"}}' $B/rpc
echo '--- empty'
curl -s -H "$H" -d '{"jsonrpc":"2.0","id":3,"method":"sleeve.spawn","params":{}}' $B/rpc
echo '--- typo field'
curl -s -H "$H" -d '{"jsonrpc":"2.0","id":4,"method":"sleeve.spawn","params":{"nme":"x"}}' $B/rpc
echo '--- list'
curl -s -H "$H" -d '{"jsonrpc":"2.0","id":5,"method":"sleeve.list"}' $B/rpc
echo '--- stream'
timeout 2.5 curl -s -N $B/events
echo '--- mcp'
curl -s $B/mcp/tools
