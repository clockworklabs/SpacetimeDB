use crate::nodes::worker_node::control_node_connection::ControlNodeClient;
use gotham::{handler::HandlerError, state::State};
use hyper::{Body, Method, Request, Response, Uri};

pub async fn proxy_to_control_node_client_api(
    mut state: State,
) -> Result<(State, Response<Body>), (State, HandlerError)> {
    let method = state.take::<Method>();
    let proxy_uri = state.take::<Uri>();
    let headers = state.take::<hyper::HeaderMap>();

    let uri = Uri::builder()
        .scheme("http") // TODO(cloutiertyler): somehow get this from gotham
        .authority(ControlNodeClient::get_shared().client_api_bootstrap_addr)
        .path_and_query(proxy_uri.path_and_query().unwrap().clone())
        .build()
        .unwrap();

    let mut builder = Request::builder().method(method).uri(&uri);

    for (header_name, header_value) in headers.iter() {
        builder = builder.header(header_name, header_value);
    }

    let request = builder.body(state.take::<Body>()).unwrap();

    let client = hyper::Client::new();
    let res = client.request(request).await.unwrap();

    Ok((state, res))
}
