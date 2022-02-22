# Set Command

echo -ne "\*3\r\n\$3\r\nSET\r\n\$3\r\nFOO\r\n\$3\r\nBAR\r\n" | netcat 127.0.0.1 6379

echo -ne "\*2\r\n\$3\r\nGET\r\n\$3\r\nFOO\r\n" | netcat 127.0.0.1 6379
