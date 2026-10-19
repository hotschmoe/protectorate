REM run from an elevated prompt
sc.exe create protectorate-node binPath= "\"C:\Program Files\protectorate\protectorate.exe\" node serve --watch \"D:\build cache\"" start= auto DisplayName= "Protectorate node"
sc.exe description protectorate-node "Protectorate node agent"
sc.exe failure protectorate-node reset= 86400 actions= restart/5000/restart/30000/restart/60000
reg.exe add HKLM\SYSTEM\CurrentControlSet\Services\protectorate-node /v Environment /t REG_MULTI_SZ /d "PROTECTORATE_HOME=C:\ProgramData\protectorate"
sc.exe start protectorate-node
