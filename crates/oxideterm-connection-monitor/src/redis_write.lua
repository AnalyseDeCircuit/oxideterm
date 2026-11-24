-- Compare the editor's baseline and publish the complete value in a single server turn.
local key, scratch = KEYS[1], KEYS[2]
local kind, mode, ttl = ARGV[1], ARGV[2], ARGV[3]
local data = cjson.decode(ARGV[4])
local base, items = data[1], data[2]
local function conflict() return "__OXIDE_VALUE_CHANGED__" end
if mode == "create" then
    if redis.call("EXISTS", key) ~= 0 then return "__OXIDE_KEY_EXISTS__" end
else
    local actual = redis.call("TYPE", key).ok
    if actual ~= kind and not (kind == "json" and actual == "ReJSON-RL") then return conflict() end
    if kind == "string" or kind == "json" then
        if kind == "string" and redis.call("STRLEN", key) ~= #base[1] then return conflict() end
        local get = kind == "json" and "JSON.GET" or "GET"
        if redis.call(get, key) ~= base[1] then return conflict() end
    elseif kind == "hash" then
        if redis.call("HLEN", key) * 2 ~= #base then return conflict() end
        for i = 1, #base, 2 do
            if redis.call("HGET", key, base[i]) ~= base[i+1] then return conflict() end
        end
    elseif kind == "set" then
        if redis.call("SCARD", key) ~= #base then return conflict() end
        for _, member in ipairs(base) do
            if redis.call("SISMEMBER", key, member) == 0 then return conflict() end
        end
    else
        local current
        if kind == "list" then
            if redis.call("LLEN", key) ~= #base then return conflict() end
            current = redis.call("LRANGE", key, 0, -1)
        else
            if redis.call("ZCARD", key) * 2 ~= #base then return conflict() end
            current = redis.call("ZRANGE", key, 0, -1, "WITHSCORES")
        end
        for i, item in ipairs(base) do if current[i] ~= item then return conflict() end end
    end
end
-- Redis scripts isolate writes but do not roll them back on error.
-- Only the final rename touches the live key; leave an existing scratch key alone.
if redis.call("EXISTS", scratch) ~= 0 then return redis.error_reply("ERR staging key exists") end
redis.call("DEL", scratch) -- Check cleanup permission before staging anything.
local cmd = ({string="SET", json="JSON.SET", list="RPUSH", hash="HSET", set="SADD", zset="ZADD"})[kind]
local args = {cmd, scratch}
if kind == "json" then table.insert(args, "$") end
for _, item in ipairs(items) do table.insert(args, item) end
local result = redis.pcall(unpack(args))
if type(result) == "table" and result.err then return result end
local function cleanup(err)
    redis.pcall("DEL", scratch)
    return err
end
if mode == "create" and ttl ~= "0" then
    result = redis.pcall("EXPIRE", scratch, ttl)
elseif mode ~= "create" then
    local remaining = redis.pcall("PTTL", key)
    if type(remaining) == "table" then return cleanup(remaining) end
    if remaining == -2 then return cleanup(conflict()) end
    if remaining >= 0 then
        if remaining == 0 then return cleanup(conflict()) end
        result = redis.pcall("PEXPIRE", scratch, remaining)
    end
end
if type(result) == "table" and result.err then return cleanup(result) end
result = redis.pcall("RENAME", scratch, key)
if result.err then return cleanup(result) end
return "OK"
