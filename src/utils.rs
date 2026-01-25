// Extracts a parameters values from an URL
pub fn extract_param_from_url(request: &str, param_name: &str) -> Result<String, String> {
    let search = format!("{param_name}=");
    let param_start = request
        .find(&search)
        .ok_or(format!("{param_name} parameter not found in callback").to_lowercase())?;

    let param_part = &request[param_start + search.len()..];
    let param_end = param_part.find(&[' ', '&'][..]).unwrap_or(param_part.len());

    Ok(param_part[..param_end].to_string())
}
