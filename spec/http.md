# HTTP:

### Table of contents:

1. general
2. connection
3. methods
4. request
   - headers
5. response
   - headers

### 1. general information:

- HTTP version - 1.1

### 2. connection:

- [x] The server accepts **persistent connections** - each connection is not closed after the first request is handled and will remain open until either the client sends a `Connection: close` header (or closes the connection otherwise), the message stalling limit is reached (time between individual requests) or the maximum number of requests sent through one connection is reached.

- [ ] The server can handle **pipelined requests** - when multiple requests are receieved from the client it handles them one by one and responds in the same order once finished.

### 3. methods:

- The following list lists all accepted HTTP request methods, other non listed methods will **not** be handled and will return a `405 METHOD_NOT_ALLOWED` status code along a list of accepted methods inside the `Accept` header.

**Accepted HTTP request methods:**

- [x] HEAD
- [ ] OPTIONS
- [x] GET
- [x] POST

### 4. request:

- Every request to the server must contain a **request line** (ex.: `GET /index.html HTTP/1.1`) containing the method used, a URI to the requested resource and the HTTP version used.
- Any URL query parameters can be specified at the end of the resources URI (ex.: `/index.html?name=john&age=21`).
- After the request line an arbitary number of request headers can be added.
- In case the POST method was used any request body data can be added after a double newline at the end of the request (newline character can be both `\n` or `\r\n`).

**Example POST request:**

```http
POST /index.php?name=john&age=21 HTTP/1.1
Content-Type: application/x-www-form-urlencoded
Accept-Encoding: gzip, deflate

username=john+doe
password=123456
```

#### Request Headers:

###### Range

- The Range header can be used to specify that partial content is being requested (usually to stream video). Only one range is accepted.
- format: `<unit>=<range-start>-<range-end>`
  - unit: the unit of meassure - **byte** only accepted
  - range-start: number of units from start
  - range-end: range-start + requested length, optional - if none specified, one **chunk** is send (default is 1MB but it can be changed inside the cree config file using the `pc_chunk_size` field)

**Example Range header usage:**

```http
# get first 512 bytes
Range: bytes=0-512

# get entire file
Range: bytes=0-
```

### 5. response:

- The response message starts with a status line containing the HTTP version and one of the following statuses based on the validity of the request and availability of the requested resource.

**Possible response status codes:**

- 200 OK
- 202 ACCEPTED
- 204 NO_CONTENT
- 206 PARTIAL_CONTENT
- 400 BAD_REQUEST
- 404 NOT_FOUND
- 405 METHOD_NOT_ALLOWED
- 401 UNAUTHORIZED
- 403 FORBIDDEN
- 500 SERVER_ERROR

for more details visit <a href="https://www.w3.org/Protocols/rfc2616/rfc2616-sec6.html#sec6.1" target="_blank">RFC2616</a>

- The response will also contain a `Date` header containing the datetime of when the response was created, `Content-type` header specifying the response body (if there is one), `Content-Encoding` header containing all compression methods used in the exact order they were used (if the `Accept-Encoding` header was set in the request) and the `Content-Length` header containing the exact number of bytes the response body after compression (if used) has.

- After all the headers a double newline will separate the response body from the headers (much like in the request the newline character can be both `\n` or `\r\n`).

**Example response message:**

```http
HTTP/1.1 200 OK

Date: Tue, 18 Jan 2022 22:00:0 GMT
Content-type: text/html
Content-Length: 13

Hello, world!
```

#### Response Headers:

###### Content-Range:

- The Content-Range header specifies what part of the requested file was sent alongside the full uncompressed file byte length.
- format: `<range-start>-<range-end>/<full-file-length>`
  - range-start: number of bytes from start
  - range-end: range-start + requested length
  - full-file-length: the entire uncompressed file length in bytes

**Example Content-Range header:**

```http
# response contains bytes 0 to 1024, the whole file is 2048 bytes long
Content-Range: 0-1024/2048
```
